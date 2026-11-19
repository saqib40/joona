//! Whisper through whisper.cpp.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use joona_protocol::{Device, Segment, Task, ms_to_samples};
use transcribe_rs::whisper_cpp::gpu::{auto_select_gpu_device, list_gpu_devices};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

/// whisper.cpp returns nothing for input shorter than a second.
const MIN_MS: u64 = 1100;
/// Language identification looks at the first 30 seconds, like Whisper itself.
const DETECT_MS: u64 = 30_000;
/// Segments Whisper itself rates as probably silent.
const NO_SPEECH_DROP: f32 = 0.8;

pub struct Whisper {
    state: WhisperState,
    _ctx: WhisperContext,
    multilingual: bool,
    pub device: Device,
    pub gpu_name: Option<String>,
}

pub struct Job<'a> {
    pub task: Task,
    pub language: Option<&'a str>,
    pub candidates: &'a [String],
    pub prompt: Option<&'a str>,
    pub threads: i32,
    pub beam_size: i32,
    pub cancel: Arc<AtomicBool>,
}

pub struct Output {
    pub text: String,
    pub language: Option<String>,
    pub language_probability: Option<f32>,
    pub segments: Vec<Segment>,
}

impl Whisper {
    pub fn load(path: &Path, use_gpu: bool) -> anyhow::Result<Self> {
        let gpus = if cfg!(any(feature = "vulkan", feature = "cuda")) {
            list_gpu_devices()
        } else {
            Vec::new()
        };
        let use_gpu = use_gpu && !gpus.is_empty();
        let gpu_device = if use_gpu { auto_select_gpu_device() } else { 0 };

        let params = WhisperContextParameters {
            use_gpu,
            flash_attn: true,
            gpu_device,
            ..Default::default()
        };
        let ctx = WhisperContext::new_with_params(path, params)
            .map_err(|e| anyhow::anyhow!("could not load {}: {e}", path.display()))?;
        let multilingual = ctx.is_multilingual();
        let state = ctx.create_state().map_err(|e| anyhow::anyhow!("{e}"))?;

        let (device, gpu_name) = if use_gpu {
            let name = gpus
                .iter()
                .find(|g| g.id == gpu_device)
                .map(|g| g.name.clone());
            (crate::GPU_BACKEND.unwrap_or(Device::Cpu), name)
        } else {
            (Device::Cpu, None)
        };
        Ok(Self {
            state,
            _ctx: ctx,
            multilingual,
            device,
            gpu_name,
        })
    }

    pub fn multilingual(&self) -> bool {
        self.multilingual
    }

    /// Runs a throwaway transcription so the first real one doesn't pay for
    /// buffer allocation (and, on GPU, shader compilation).
    pub fn warm_up(&mut self, threads: i32) {
        let silence = vec![0.0f32; ms_to_samples(MIN_MS)];
        let mut params = base_params(threads, 1);
        params.set_language(Some("en"));
        let _ = self.state.full(params, &silence);
    }

    pub fn transcribe(&mut self, audio: &[f32], job: &Job) -> anyhow::Result<Output> {
        let min = ms_to_samples(MIN_MS);
        let padded;
        let audio = if audio.len() < min {
            let mut v = audio.to_vec();
            v.resize(min, 0.0);
            padded = v;
            &padded[..]
        } else {
            audio
        };

        let (language, language_probability) = if !self.multilingual {
            (Some("en".to_string()), None)
        } else if let Some(l) = job.language {
            (Some(l.to_string()), None)
        } else if !job.candidates.is_empty() {
            match self.detect_among(audio, job.candidates, job.threads)? {
                Some((l, p)) => (Some(l), Some(p)),
                None => (None, None),
            }
        } else {
            (None, None)
        };

        if job.task == Task::Translate && !self.multilingual && language.as_deref() != Some("en") {
            anyhow::bail!(crate::Unsupported(
                "this Whisper model is English-only and can't translate".into()
            ));
        }

        let mut params = base_params(job.threads, job.beam_size);
        let translate = job.task == Task::Translate && language.as_deref() != Some("en");
        params.set_language(Some(language.as_deref().unwrap_or("auto")));
        params.set_translate(translate);
        let writes = if translate {
            Some("en")
        } else {
            language.as_deref()
        };
        if let Some(prompt) = job.prompt.and_then(|p| prompt_for(p, writes)) {
            params.set_initial_prompt(&prompt);
        }
        // `set_abort_callback_safe` in whisper-rs 0.16 casts its user data to
        // the wrong type, so the flag is wired up through the raw API instead.
        // `job.cancel` outlives the call to `full`.
        unsafe {
            params.set_abort_callback(Some(abort_requested));
            params.set_abort_callback_user_data(Arc::as_ptr(&job.cancel) as *mut c_void);
        }

        let result = self.state.full(params, audio);
        if job.cancel.load(Ordering::Relaxed) {
            anyhow::bail!(crate::Cancelled);
        }
        result.map_err(|e| anyhow::anyhow!("whisper failed: {e}"))?;

        let detected = match &language {
            Some(l) => Some(l.clone()),
            None => {
                whisper_rs::get_lang_str(self.state.full_lang_id_from_state()).map(str::to_owned)
            }
        };

        let mut text = String::new();
        let mut segments = Vec::new();
        for i in 0..self.state.full_n_segments() {
            let Some(seg) = self.state.get_segment(i) else {
                continue;
            };
            if seg.no_speech_probability() > NO_SPEECH_DROP {
                continue;
            }
            let Ok(s) = seg.to_str_lossy() else { continue };
            text.push_str(&s);
            segments.push(Segment {
                start_ms: (seg.start_timestamp().max(0) as u64) * 10,
                end_ms: (seg.end_timestamp().max(0) as u64) * 10,
                text: s.trim().to_string(),
            });
        }
        Ok(Output {
            text: text.trim().to_string(),
            language: detected,
            language_probability,
            segments,
        })
    }

    /// Picks the most likely of the user's languages. Plain auto-detection
    /// often hears Hindi as Urdu or Marathi as Hindi; limiting it to the
    /// languages someone actually speaks removes most of those mistakes.
    fn detect_among(
        &mut self,
        audio: &[f32],
        candidates: &[String],
        threads: i32,
    ) -> anyhow::Result<Option<(String, f32)>> {
        let threads = threads.max(1) as usize;
        let window = &audio[..audio.len().min(ms_to_samples(DETECT_MS))];
        self.state
            .pcm_to_mel(window, threads)
            .map_err(|e| anyhow::anyhow!("language detection failed: {e}"))?;
        let (_, probs) = self
            .state
            .lang_detect(0, threads)
            .map_err(|e| anyhow::anyhow!("language detection failed: {e}"))?;

        let mut best: Option<(&String, f32)> = None;
        let mut total = 0.0;
        for c in candidates {
            let Some(id) = whisper_rs::get_lang_id(c) else {
                continue;
            };
            let p = probs.get(id as usize).copied().unwrap_or(0.0);
            total += p;
            if best.is_none_or(|(_, bp)| p > bp) {
                best = Some((c, p));
            }
        }
        Ok(best.map(|(l, p)| (l.clone(), if total > 0.0 { p / total } else { 0.0 })))
    }
}

/// Languages Whisper writes in Latin letters.
const LATIN: &[&str] = &[
    "en", "fr", "es", "de", "it", "pt", "nl", "pl", "ro", "cs", "sk", "sl", "hr", "bs", "sv", "da",
    "no", "nn", "fi", "et", "lv", "lt", "hu", "tr", "az", "uz", "id", "ms", "tl", "vi", "sw", "ca",
    "eu", "gl", "cy", "is", "mt", "af", "sq", "lb", "la", "haw", "ln", "sn", "so", "yo", "ha",
    "jw", "su", "mi", "oc", "br", "fo", "ht", "tk",
];

/// The words of a comma-separated prompt that are written in the script
/// Whisper will write in. English names would push Hindi into Latin
/// letters, and Hindi words would pull English out of them.
fn prompt_for(prompt: &str, writes: Option<&str>) -> Option<String> {
    let latin = writes.is_none_or(|l| LATIN.contains(&l));
    let words: Vec<&str> = prompt
        .split(',')
        .map(str::trim)
        .filter(|w| !w.is_empty() && !w.chars().any(char::is_control) && is_latin(w) == latin)
        .collect();
    (!words.is_empty()).then(|| words.join(", "))
}

fn is_latin(word: &str) -> bool {
    word.chars()
        .filter(|c| c.is_alphabetic())
        .all(|c| (c as u32) < 0x0250 || (0x1E00..0x1F00).contains(&(c as u32)))
}

unsafe extern "C" fn abort_requested(user_data: *mut c_void) -> bool {
    // SAFETY: the user data is the `AtomicBool` inside the job's cancel flag.
    unsafe { (*(user_data as *const AtomicBool)).load(Ordering::Relaxed) }
}

fn base_params<'a, 'b>(threads: i32, beam_size: i32) -> FullParams<'a, 'b> {
    let mut p = if beam_size > 1 {
        FullParams::new(SamplingStrategy::BeamSearch {
            beam_size,
            patience: -1.0,
        })
    } else {
        FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
    };
    p.set_n_threads(threads);
    // Each dictation stands alone; carrying text over invites repeated phrases.
    p.set_no_context(true);
    p.set_print_special(false);
    p.set_print_progress(false);
    p.set_print_realtime(false);
    p.set_print_timestamps(false);
    p.set_suppress_blank(true);
    p.set_suppress_nst(true);
    p.set_temperature(0.0);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_keep_words_in_the_script_whisper_writes() {
        let prompt = "Priya, Joona, प्रिया, Café Noir, , Bad\0Word";
        assert_eq!(
            prompt_for(prompt, Some("en")).as_deref(),
            Some("Priya, Joona, Café Noir")
        );
        assert_eq!(
            prompt_for(prompt, None).as_deref(),
            Some("Priya, Joona, Café Noir")
        );
        assert_eq!(prompt_for(prompt, Some("hi")).as_deref(), Some("प्रिया"));
        assert_eq!(prompt_for("Priya, Joona", Some("ta")), None);
    }
}
