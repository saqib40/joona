//! The dictation controller.
//!
//! One task owns every dictation's state and handles commands in order:
//! gestures from the keyboard hook, clicks on the bar, and reports from the
//! slow work it starts (opening the microphone, loading a model,
//! transcribing, pasting). That work runs elsewhere and reports back, so
//! the controller never blocks and each state change happens in one place.
//!
//! Audio doesn't pass through the controller: the capture thread appends it
//! to the dictation's [`Tap`] and streams it to the engine directly.
//!
//! A recording Sarvam or the user's own speech-to-text service should hear
//! goes there once it's complete. When the service can't be reached, the
//! model on this PC takes over.
//!
//! In Command Mode the words are an instruction for the AI model, which
//! works on the text selected in the app, or writes at the cursor.

mod ui;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use joona_protocol::{Task, TranscribeResult};
use tauri::async_runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub use self::ui::{
    BarState, Done, Listening, Processing, Toast, ToastAction, ToastButton, Tone, Ui,
};
use crate::ai::{self, Ask};
use crate::audio::{
    self, AudioSink, Capture, Limit, Recording, SAMPLE_RATE, encode_wav, samples_to_ms,
};
use crate::history::{self, Delivery, Entry};
use crate::hotkeys::{Action, CancelReason, Mode};
use crate::insert::{self, AppOverride, CopyReason, Method, Outcome};
use crate::languages;
use crate::pipeline::style::app_name;
use crate::pipeline::{self, Category};
use crate::providers::llm::KeyNeed;
use crate::providers::sarvam::{self, Sarvam};
use crate::providers::stt::{self, Listen, Stt};
use crate::providers::{self, ProviderError, SARVAM};
use crate::router::{self, Handoff, Listener, Local, Rewrite, Route, RouteError, RouteLabel};
use crate::settings::{PolishLevel, Script, Settings};
use crate::sounds::Cue;
use crate::state::AppState;
use crate::supervisor::{self, EngineError};

/// Shorter recordings are accidental taps.
const MIN_AUDIO_MS: u64 = 300;
/// The bar and the start sound wait this long, so shortcuts that begin with
/// the trigger (Ctrl+Win+Left, Ctrl+Win+D) don't flash the bar.
const ARMING: Duration = Duration::from_millis(120);
/// Audio goes to the engine in chunks of at least this many samples.
const FEED_CHUNK: usize = SAMPLE_RATE as usize / 10;
/// How long the bar shows its check mark after the text went in.
const DONE: Duration = Duration::from_millis(900);
const UNDO_TOAST_MS: u32 = 6_000;
/// Audio kept for Undo or Retry is dropped after this long.
const KEEP_AUDIO: Duration = Duration::from_secs(60);
/// The id used for "Paste last transcript", which has no dictation.
const PASTE_LAST: u64 = 0;
/// Gives the app a moment to take in the text before "press enter" sends it.
const ENTER_DELAY: Duration = Duration::from_millis(60);

pub enum Command {
    Gesture(Action),
    /// A click on the bar, or "Start hands-free" in the tray.
    StartHandsFree,
    /// The bar's stop button.
    Stop,
    /// The bar's cancel button.
    Cancel,
    ToastAction {
        toast: u32,
        action: ToastAction,
    },
    /// A toast from outside a dictation, such as a finished download.
    Toast(Toast),
    PasteLast,
    /// Pastes text from the history into the app in front.
    Paste(String),

    Armed {
        id: u64,
    },
    CaptureStarted {
        id: u64,
        result: Result<Capture, String>,
    },
    CaptureStopped {
        id: u64,
    },
    MicFailed {
        id: u64,
        message: String,
    },
    Limit {
        id: u64,
        limit: Limit,
    },
    EngineOpened {
        id: u64,
        generation: u32,
        result: Result<supervisor::Session, EngineError>,
    },
    Transcribed {
        id: u64,
        result: Result<TranscribeResult, EngineError>,
    },
    CloudHeard {
        id: u64,
        result: Result<Transcript, ProviderError>,
    },
    /// The text selected when a command was spoken, if any.
    SelectionRead {
        id: u64,
        text: Option<String>,
    },
    /// The AI model carried out a spoken command.
    Commanded {
        id: u64,
        result: Result<String, ProviderError>,
    },
    Rewritten {
        id: u64,
        rewrite: Rewrite,
        /// The cloud service that did it.
        by: Option<String>,
        result: Result<String, ProviderError>,
    },
    /// The AI model polished the text, and translated it when asked.
    Polished {
        id: u64,
        result: Result<String, ProviderError>,
    },
    Inserted {
        id: u64,
        app: Option<String>,
        result: Result<Outcome, InsertFailure>,
    },
    DoneShown {
        id: u64,
    },
    ForgetAudio {
        toast: u32,
    },
}

pub struct InsertFailure {
    message: String,
    copied: bool,
}

/// What a cloud service made of a recording.
pub enum Transcript {
    Sarvam(sarvam::Heard),
    Byok(stt::Heard),
}

#[derive(Clone)]
pub struct SessionHandle(UnboundedSender<Command>);

impl SessionHandle {
    pub fn channel() -> (Self, UnboundedReceiver<Command>) {
        let (tx, rx) = unbounded_channel();
        (Self(tx), rx)
    }

    pub fn send(&self, command: Command) {
        let _ = self.0.send(command);
    }

    fn send_after(&self, delay: Duration, command: Command) {
        let tx = self.clone();
        async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            tx.send(command);
        });
    }
}

pub fn spawn(state: Arc<AppState>, rx: UnboundedReceiver<Command>) {
    let controller = Controller {
        session: state.session.clone(),
        state,
        next_id: PASTE_LAST,
        next_toast: 0,
        dictations: HashMap::new(),
        current: None,
        kept: None,
        always_type: None,
        last_text: None,
        told_untranslated: false,
        told_no_ai: false,
        bar: BarState::Resting,
    };
    async_runtime::spawn(controller.run(rx));
}

/// The audio of one dictation, shared with the capture thread.
struct Tap {
    recording: Recording,
    engine: Engine,
    /// Bumped whenever the engine session is replaced, so a late answer from
    /// an earlier `open` is ignored.
    generation: u32,
}

enum Engine {
    Opening,
    Open(supervisor::Session),
    Failed(EngineError),
    /// Finishing, not needed, or not used because Sarvam hears this one.
    Closed,
}

impl Tap {
    fn new(recording: Recording, engine: Engine) -> Arc<Mutex<Tap>> {
        Arc::new(Mutex::new(Tap {
            recording,
            engine,
            generation: 0,
        }))
    }

    /// Starts a new engine session that will hear the whole recording.
    fn reopen(&mut self) -> u32 {
        self.generation += 1;
        self.engine = Engine::Opening;
        self.recording.rewind();
        self.generation
    }

    fn stream(&mut self, everything: bool) {
        if let Engine::Open(session) = &self.engine
            && (everything || self.recording.unstreamed_len() >= FEED_CHUNK)
        {
            let _ = session.feed(self.recording.take_unstreamed());
        }
    }
}

type SharedTap = Arc<Mutex<Tap>>;

fn lock(tap: &SharedTap) -> MutexGuard<'_, Tap> {
    tap.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
struct Sink {
    id: u64,
    tap: SharedTap,
    session: SessionHandle,
    ui: Arc<dyn Ui>,
    last_limit: Limit,
}

impl AudioSink for Sink {
    fn on_audio(&mut self, samples: &[f32]) {
        let limit = {
            let mut tap = lock(&self.tap);
            let limit = tap.recording.push(samples);
            tap.stream(false);
            limit
        };
        if limit != Limit::None && limit != self.last_limit {
            self.last_limit = limit;
            self.session.send(Command::Limit { id: self.id, limit });
        }
    }

    fn on_level(&mut self, level: f32) {
        self.ui.level(level);
    }

    fn on_error(&mut self, message: String) {
        self.session.send(Command::MicFailed {
            id: self.id,
            message,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Recording,
    /// The microphone is closing; the last audio is on its way.
    Stopping,
    /// All audio is in, but the engine session is still opening.
    WaitingForEngine,
    Transcribing,
    /// Sarvam or the user's own service is turning the recording into text.
    Cloud,
    /// Sarvam or the AI model is translating or romanizing the transcript.
    Rewriting,
    /// The AI model is polishing the text, and translating it when asked.
    Polishing,
    /// A command was heard; the selected text is still being read.
    Selecting,
    /// The AI model is carrying out a command.
    Commanding,
    Inserting,
    Done(Outcome),
    /// Cancelled. The microphone is closing, and the audio is kept for Undo.
    Discarding,
}

struct Dictation {
    mode: Mode,
    hands_free: bool,
    started: Instant,
    armed: bool,
    route: Route,
    tap: SharedTap,
    capture: Option<Capture>,
    stage: Stage,
    ending_soon: bool,
    text: String,
    heard: Option<Heard>,
    /// What the model here heard while a cloud service is asked, to fall
    /// back on.
    local: Option<TranscribeResult>,
    /// The cloud service failed and the model here took over.
    fell_back: bool,
    /// Words the text pipeline changed.
    fixes: u32,
    /// The kind of app the text went into.
    category: Option<Category>,
    /// The cloud service working on the text now, for the badge.
    working: Option<String>,
    /// The text the AI model is polishing, as the pipeline prepared it.
    prepared: Option<pipeline::Prepared>,
    /// What the AI model was asked to do as well as polishing.
    folded: Option<Rewrite>,
    /// The text a command works on.
    selection: Selection,
}

impl Dictation {
    fn new(mode: Mode, route: Route, tap: SharedTap, stage: Stage) -> Self {
        Dictation {
            mode,
            hands_free: false,
            started: Instant::now(),
            armed: false,
            route,
            tap,
            capture: None,
            stage,
            ending_soon: false,
            text: String::new(),
            heard: None,
            local: None,
            fell_back: false,
            fixes: 0,
            category: None,
            working: None,
            prepared: None,
            folded: None,
            selection: Selection::Unread,
        }
    }
}

/// The text selected in the app when a command was spoken.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Selection {
    /// Not read, as for a dictation.
    Unread,
    Reading,
    Read(Option<String>),
}

/// A transcript, kept for the history entry.
struct Heard {
    /// The text as heard, before translation and cleanup.
    raw: String,
    /// The spoken language.
    language: Option<String>,
    /// The language `raw` or the typed text is in, when that isn't the
    /// spoken one.
    output: Option<String>,
    /// `raw` is already in Latin letters.
    romanized: bool,
    /// Any speech was heard at all.
    speech: bool,
    audio_ms: u32,
    model: String,
    /// The cloud service that heard or rewrote it.
    cloud: Option<String>,
}

impl Heard {
    fn local(result: &TranscribeResult, route: &Route) -> Self {
        let language = result
            .language
            .clone()
            .or_else(|| match route.spoken.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            });
        let translated = route
            .local
            .as_ref()
            .is_some_and(|l| l.open.task == Task::Translate);
        Heard {
            raw: result.text.trim().to_owned(),
            output: translated
                .then(|| route.output.clone())
                .flatten()
                .filter(|out| language.as_ref() != Some(out)),
            language,
            romanized: false,
            speech: result.speech_ms > 0,
            audio_ms: result.audio_ms.min(u64::from(u32::MAX)) as u32,
            model: result.model.clone(),
            cloud: None,
        }
    }

    fn sarvam(heard: sarvam::Heard, mode: sarvam::Mode, route: &Route, audio_ms: u64) -> Self {
        Heard {
            output: (mode == sarvam::Mode::Translate)
                .then(|| route.output.clone())
                .flatten()
                .filter(|out| heard.language.as_ref() != Some(out)),
            raw: heard.text,
            language: heard.language,
            romanized: mode == sarvam::Mode::Translit,
            speech: true,
            audio_ms: audio_ms.min(u64::from(u32::MAX)) as u32,
            model: sarvam::STT_MODEL.to_owned(),
            cloud: Some(sarvam::NAME.to_owned()),
        }
    }

    /// From the speech-to-text service the user brought a key for.
    fn byok(heard: stt::Heard, route: &Route, audio_ms: u64) -> Self {
        let service = route.byok.as_ref();
        let language = heard
            .language
            .or_else(|| languages::guess(&heard.text, &route.spoken));
        Heard {
            output: heard
                .translated
                .then(|| route.output.clone())
                .flatten()
                .filter(|out| language.as_ref() != Some(out)),
            raw: heard.text.trim().to_owned(),
            language,
            romanized: false,
            speech: true,
            audio_ms: audio_ms.min(u64::from(u32::MAX)) as u32,
            model: service.map_or_else(String::new, |s| s.model.clone()),
            cloud: service.filter(|s| s.cloud).map(|s| s.spec.name.to_owned()),
        }
    }
}

/// Why a dictation failed, and what its toast can offer.
struct Failure {
    title: &'static str,
    message: String,
    /// The fix is on the Providers page.
    account: bool,
    /// The model here could transcribe it instead.
    local: bool,
}

impl Failure {
    fn engine(error: impl std::fmt::Display) -> Self {
        Failure {
            title: "Couldn't transcribe",
            message: error.to_string(),
            account: false,
            local: false,
        }
    }
}

/// The audio of a cancelled or failed dictation, for Undo or Retry.
struct Kept {
    toast: u32,
    samples: Vec<f32>,
    mode: Mode,
    route: Route,
}

struct Controller {
    state: Arc<AppState>,
    session: SessionHandle,
    next_id: u64,
    next_toast: u32,
    dictations: HashMap<u64, Dictation>,
    /// The dictation that is listening, if any.
    current: Option<u64>,
    kept: Option<Kept>,
    /// The app offered in an "Always type in …" toast.
    always_type: Option<(u32, String)>,
    last_text: Option<String>,
    /// The user was told text can't be translated or romanized yet. Once is
    /// enough; the Languages page says so too.
    told_untranslated: bool,
    /// The user was told Smart polish isn't set up, so Basic polish is used.
    told_no_ai: bool,
    bar: BarState,
}

impl Controller {
    async fn run(mut self, mut rx: UnboundedReceiver<Command>) {
        while let Some(command) = rx.recv().await {
            self.handle(command);
            self.refresh_bar();
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Gesture(action) => self.gesture(action),
            Command::StartHandsFree => self.start(Mode::Dictate, true),
            Command::Stop => self.stop(),
            Command::Cancel => self.cancel(CancelReason::Escape),
            Command::ToastAction { toast, action } => self.toast_action(toast, action),
            Command::Toast(toast) => {
                self.toast(toast);
            }
            Command::PasteLast => self.paste_last(),
            Command::Paste(mut text) => {
                if self.state.settings.get().insertion.trailing_space {
                    text.push(' ');
                }
                self.insert(PASTE_LAST, text, false);
            }
            Command::Armed { id } => self.armed(id),
            Command::CaptureStarted { id, result } => self.capture_started(id, result),
            Command::CaptureStopped { id } => self.capture_stopped(id),
            Command::MicFailed { id, message } => self.mic_failed(id, message),
            Command::Limit { id, limit } => self.limit(id, limit),
            Command::EngineOpened {
                id,
                generation,
                result,
            } => self.engine_opened(id, generation, result),
            Command::Transcribed { id, result } => self.transcribed(id, result),
            Command::CloudHeard { id, result } => self.cloud_heard(id, result),
            Command::SelectionRead { id, text } => self.selection_read(id, text),
            Command::Commanded { id, result } => self.commanded(id, result),
            Command::Rewritten {
                id,
                rewrite,
                by,
                result,
            } => self.rewritten(id, rewrite, by, result),
            Command::Polished { id, result } => self.polished(id, result),
            Command::Inserted { id, app, result } => self.inserted(id, app, result),
            Command::DoneShown { id } => {
                if matches!(self.stage(id), Some(Stage::Done(_))) {
                    self.dictations.remove(&id);
                }
            }
            Command::ForgetAudio { toast } => {
                if self.kept.as_ref().is_some_and(|k| k.toast == toast) {
                    self.kept = None;
                }
            }
        }
    }

    fn gesture(&mut self, action: Action) {
        match action {
            Action::Start { mode } => self.start(mode, false),
            Action::ModeChanged { mode } => self.change_mode(mode),
            Action::HandsFree => {
                if let Some(d) = self.current.and_then(|id| self.dictations.get_mut(&id)) {
                    d.hands_free = true;
                }
            }
            Action::Stop => self.stop(),
            Action::Cancel { reason } => self.cancel(reason),
            Action::PasteLast => self.paste_last(),
            Action::InjectMask => {}
        }
    }

    // Recording

    fn start(&mut self, mode: Mode, from_bar: bool) {
        if self.current.is_some() {
            // The shortcut ends a hands-free dictation that the bar started.
            if !from_bar {
                self.stop();
            }
            return;
        }
        let settings = self.state.settings.get();
        let route = match router::route(&settings, mode, &self.state.library) {
            Ok(route) => route,
            Err(e) => {
                self.end_gesture();
                self.route_failed(e);
                return;
            }
        };
        if mode == Mode::Command
            && let Err(why) = self.ai(&settings)
        {
            self.end_gesture();
            return self.command_needs_ai(&why);
        }
        self.forget_kept();

        self.next_id += 1;
        let id = self.next_id;
        let listener = listener(&route).cloned();
        let engine = match listener {
            Some(_) => Engine::Opening,
            None => Engine::Closed,
        };
        let tap = Tap::new(Recording::new(), engine);
        crate::audio::ensure_mic_access();
        self.open_mic(id, &tap, settings.general.microphone.clone());
        if let Some(local) = &listener {
            self.open_engine(id, 0, local);
        }
        self.warm_up_ai(&settings, &route, mode);
        let mut dictation = Dictation::new(mode, route, tap, Stage::Recording);
        dictation.hands_free = from_bar;
        dictation.armed = from_bar;
        self.dictations.insert(id, dictation);
        self.current = Some(id);
        if from_bar {
            self.cue(Cue::Start);
            self.with_hotkeys(|h| h.begin_hands_free());
        } else {
            self.session.send_after(ARMING, Command::Armed { id });
        }
    }

    fn armed(&mut self, id: u64) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        if d.stage == Stage::Recording && !d.armed {
            d.armed = true;
            self.cue(Cue::Start);
        }
    }

    fn open_mic(&self, id: u64, tap: &SharedTap, device: Option<String>) {
        let sink = Sink {
            id,
            tap: tap.clone(),
            session: self.session.clone(),
            ui: self.state.ui.clone(),
            last_limit: Limit::None,
        };
        let session = self.session.clone();
        async_runtime::spawn_blocking(move || {
            let result = match Capture::start(device.as_deref(), Box::new(sink.clone())) {
                Ok(capture) => Ok(capture),
                Err(e) if device.is_some() => {
                    tracing::warn!("the chosen microphone failed ({e:#}); trying the default one");
                    Capture::start(None, Box::new(sink)).map_err(|e| format!("{e:#}"))
                }
                Err(e) => Err(format!("{e:#}")),
            };
            if let Ok(capture) = &result {
                tracing::debug!(id, device = %capture.info.device_name, rate = capture.info.sample_rate, "microphone open");
            }
            session.send(Command::CaptureStarted { id, result });
        });
    }

    fn close_mic(&self, id: u64, capture: Capture) {
        let session = self.session.clone();
        async_runtime::spawn_blocking(move || {
            capture.stop();
            session.send(Command::CaptureStopped { id });
        });
    }

    fn capture_started(&mut self, id: u64, result: Result<Capture, String>) {
        let stage = self.stage(id);
        match result {
            Ok(capture) => match stage {
                Some(Stage::Recording) => {
                    if let Some(d) = self.dictations.get_mut(&id) {
                        d.capture = Some(capture);
                    }
                }
                // Stopped or cancelled while the microphone was opening.
                Some(_) => self.close_mic(id, capture),
                None => {
                    async_runtime::spawn_blocking(move || capture.stop());
                }
            },
            Err(message) => {
                if stage.is_none() {
                    return;
                }
                tracing::warn!(id, "couldn't open the microphone: {message}");
                self.dictations.remove(&id);
                if self.current == Some(id) {
                    self.current = None;
                    self.end_gesture();
                }
                self.cue(Cue::Error);
                self.toast(
                    Toast::new(Tone::Error, "Couldn't open your microphone")
                        .detail(message)
                        .action(ToastAction::OpenMicSettings, "Microphone settings"),
                );
            }
        }
    }

    fn stop(&mut self) {
        let Some(id) = self.current.take() else {
            return;
        };
        self.end_gesture();
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.stage = Stage::Stopping;
        let capture = d.capture.take();
        let heard = d.armed;
        let command = d.mode == Mode::Command;
        if let Some(capture) = capture {
            self.close_mic(id, capture);
        }
        if heard {
            self.cue(Cue::Stop);
            if command {
                self.read_selection(id);
            }
        }
    }

    fn cancel(&mut self, reason: CancelReason) {
        let Some(id) = self.current.take() else {
            return;
        };
        self.end_gesture();
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        lock(&d.tap).engine = Engine::Closed;
        let capture = d.capture.take();
        if reason == CancelReason::Escape {
            d.stage = Stage::Discarding;
            if let Some(capture) = capture {
                self.close_mic(id, capture);
            }
            self.cue(Cue::Cancel);
        } else {
            // A tap or a shortcut such as Ctrl+Win+D: nothing to show.
            self.dictations.remove(&id);
            if let Some(capture) = capture {
                async_runtime::spawn_blocking(move || capture.stop());
            }
        }
    }

    fn capture_stopped(&mut self, id: u64) {
        match self.stage(id) {
            Some(Stage::Stopping) => self.finalize(id),
            Some(Stage::Discarding) => self.discarded(id),
            _ => {}
        }
    }

    fn mic_failed(&mut self, id: u64, message: String) {
        if self.current != Some(id) {
            return;
        }
        tracing::warn!(id, "the microphone failed: {message}");
        self.stop();
        self.toast(Toast::new(Tone::Warning, "The microphone stopped").detail(message));
    }

    fn limit(&mut self, id: u64, limit: Limit) {
        if self.current != Some(id) {
            return;
        }
        match limit {
            Limit::Warn => {
                if let Some(d) = self.dictations.get_mut(&id) {
                    d.ending_soon = true;
                }
                self.toast(
                    Toast::new(Tone::Warning, "One minute left")
                        .detail("Dictations stop after 20 minutes."),
                );
            }
            Limit::Reached => {
                self.stop();
                self.toast(
                    Toast::new(Tone::Neutral, "Stopped at 20 minutes")
                        .detail("Transcribing what you said."),
                );
            }
            Limit::None => {}
        }
    }

    fn change_mode(&mut self, mode: Mode) {
        let Some(id) = self.current else {
            return;
        };
        let settings = self.state.settings.get();
        let Ok(route) = router::route(&settings, mode, &self.state.library) else {
            return;
        };
        self.warm_up_ai(&settings, &route, mode);
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.mode = mode;
        let before = listener(&d.route).cloned();
        let after = listener(&route).cloned();
        d.route = route;
        if before == after {
            return;
        }
        // Speech already recorded goes to a fresh session with the new settings.
        let generation = {
            let mut tap = lock(&d.tap);
            let generation = tap.reopen();
            if after.is_none() {
                tap.engine = Engine::Closed;
            }
            generation
        };
        if let Some(local) = &after {
            self.open_engine(id, generation, local);
        }
    }

    // Transcription

    fn open_engine(&self, id: u64, generation: u32, local: &Local) {
        let engine = self.state.engine.clone();
        let session = self.session.clone();
        let (model, use_gpu, mut params) = (local.model.clone(), local.use_gpu, local.open.clone());
        if params.prompt.is_none() {
            params.prompt = self.state.rules().vocabulary.prompt();
        }
        async_runtime::spawn(async move {
            let result = async {
                engine.load(model, use_gpu).await?;
                engine.open(params).await
            }
            .await;
            session.send(Command::EngineOpened {
                id,
                generation,
                result,
            });
        });
    }

    fn engine_opened(
        &mut self,
        id: u64,
        generation: u32,
        result: Result<supervisor::Session, EngineError>,
    ) {
        let Some(d) = self.dictations.get(&id) else {
            return;
        };
        let stage = d.stage;
        let failed = {
            let mut tap = lock(&d.tap);
            if tap.generation != generation || !matches!(tap.engine, Engine::Opening) {
                return;
            }
            match result {
                Ok(session) => {
                    tap.engine = Engine::Open(session);
                    tap.stream(true);
                    false
                }
                Err(e) => {
                    tracing::warn!(id, "the engine couldn't start this dictation: {e}");
                    tap.engine = Engine::Failed(e);
                    true
                }
            }
        };
        match stage {
            Stage::WaitingForEngine => self.transcribe(id),
            // No point in recording more: say so now rather than at the end.
            Stage::Recording if failed && self.current == Some(id) => self.stop(),
            _ => {}
        }
    }

    fn finalize(&mut self, id: u64) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let (duration, muted, engine_ready) = {
            let tap = lock(&d.tap);
            (
                tap.recording.duration_ms(),
                tap.recording.sounds_muted(),
                !matches!(tap.engine, Engine::Opening),
            )
        };
        if duration < MIN_AUDIO_MS {
            self.dictations.remove(&id);
            return;
        }
        if muted {
            self.dictations.remove(&id);
            let toast = if audio::mic_access().blocked() {
                #[cfg(target_os = "macos")]
                let (title, detail) = (
                    "macOS is blocking your microphone",
                    "Turn on microphone access for Joona in System Settings › Privacy & Security › Microphone.",
                );
                #[cfg(not(target_os = "macos"))]
                let (title, detail) = (
                    "Windows is blocking your microphone",
                    "Turn on microphone access for desktop apps in Windows Settings.",
                );
                Toast::new(Tone::Warning, title)
                    .detail(detail)
                    .action(ToastAction::OpenMicSettings, "Open Settings")
            } else {
                Toast::new(Tone::Warning, "Your microphone seems muted")
                    .detail(
                        "Joona heard only silence. Check that the right microphone is selected.",
                    )
                    .action(ToastAction::OpenMicSettings, "Microphone settings")
            };
            self.toast(toast);
            return;
        }
        d.stage = Stage::WaitingForEngine;
        if listener(&d.route).is_none() {
            match d.route.handoff(None) {
                Some(handoff) => self.ask_cloud(id, handoff),
                None => {
                    self.dictations.remove(&id);
                }
            }
            return;
        }
        if engine_ready {
            self.transcribe(id);
        }
    }

    fn transcribe(&mut self, id: u64) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let mut tap = lock(&d.tap);
        match std::mem::replace(&mut tap.engine, Engine::Closed) {
            Engine::Open(session) => {
                let _ = session.feed(tap.recording.take_unstreamed());
                let timeout = supervisor::finish_timeout(tap.recording.duration_ms());
                drop(tap);
                d.stage = Stage::Transcribing;
                let tx = self.session.clone();
                async_runtime::spawn(async move {
                    let result = session.finish(timeout).await;
                    tx.send(Command::Transcribed { id, result });
                });
            }
            Engine::Failed(error) => {
                drop(tap);
                self.fail(id, Failure::engine(error));
            }
            Engine::Opening => tap.engine = Engine::Opening,
            Engine::Closed => {}
        }
    }

    fn transcribed(&mut self, id: u64, result: Result<TranscribeResult, EngineError>) {
        let result = match result {
            Ok(result) => result,
            Err(e) => return self.fail(id, Failure::engine(e)),
        };
        tracing::info!(
            id,
            model = %result.model,
            audio_ms = result.audio_ms,
            speech_ms = result.speech_ms,
            infer_ms = result.infer_ms,
            language = ?result.language,
            "transcribed"
        );
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let handoff = match d.fell_back || result.text.trim().is_empty() {
            true => None,
            false => d.route.handoff(result.language.as_deref()),
        };
        if let Some(handoff) = handoff {
            d.local = Some(result);
            return self.ask_cloud(id, handoff);
        }
        let heard = Heard::local(&result, &d.route);
        self.got_text(id, heard);
    }

    /// Sends the whole recording to Sarvam, or to the user's own service.
    fn ask_cloud(&mut self, id: u64, handoff: Handoff) {
        let terms = self.state.rules().vocabulary.terms().to_vec();
        let keys = self.state.keys.clone();
        let tx = self.session.clone();
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let samples = lock(&d.tap).recording.samples().to_vec();
        match handoff.to {
            Listener::Sarvam => {
                let Some(mode) = d.route.sarvam.as_ref().map(|s| s.mode) else {
                    return;
                };
                d.stage = Stage::Cloud;
                d.working = Some(sarvam::NAME.to_owned());
                tracing::debug!(id, language = ?handoff.language, ?mode, "asking Sarvam");
                async_runtime::spawn(async move {
                    let result = async {
                        let sarvam = Sarvam::new(providers::key(&keys, SARVAM).await?);
                        sarvam
                            .transcribe(samples, handoff.language.as_deref(), mode, &terms)
                            .await
                    }
                    .await;
                    tx.send(Command::CloudHeard {
                        id,
                        result: result.map(Transcript::Sarvam),
                    });
                });
            }
            Listener::Byok => {
                let Some(service) = d.route.byok.clone() else {
                    return;
                };
                d.stage = Stage::Cloud;
                d.working = service.cloud.then(|| service.spec.name.to_owned());
                // Without a language to listen for, the service picks among
                // the ones the user speaks.
                let candidates = match handoff.language {
                    Some(_) => Vec::new(),
                    None => d.route.spoken.clone(),
                };
                let to_english = service.translate;
                tracing::debug!(id, provider = service.spec.id, model = %service.model, language = ?handoff.language, "asking the speech-to-text service");
                async_runtime::spawn(async move {
                    let spec = service.spec;
                    let result = async {
                        let key = match spec.key {
                            KeyNeed::Never => None,
                            KeyNeed::Optional => providers::key(&keys, spec.id).await.ok(),
                            KeyNeed::Required => Some(providers::key(&keys, spec.id).await?),
                        };
                        let listen = Listen {
                            language: handoff.language.as_deref(),
                            candidates: &candidates,
                            to_english,
                            terms: &terms,
                        };
                        Stt::new(spec, service.base, key, service.model)
                            .transcribe(samples, listen)
                            .await
                    }
                    .await;
                    tx.send(Command::CloudHeard {
                        id,
                        result: result.map(Transcript::Byok),
                    });
                });
            }
        }
    }

    fn cloud_heard(&mut self, id: u64, result: Result<Transcript, ProviderError>) {
        if self.stage(id) != Some(Stage::Cloud) {
            return;
        }
        let transcript = match result {
            Ok(transcript) => transcript,
            Err(e) => return self.cloud_failed(id, e),
        };
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.local = None;
        d.working = None;
        let audio_ms = lock(&d.tap).recording.duration_ms();
        let heard = match transcript {
            Transcript::Sarvam(heard) => {
                let mode = d
                    .route
                    .sarvam
                    .as_ref()
                    .map_or(sarvam::Mode::Transcribe, |s| s.mode);
                tracing::info!(id, audio_ms, language = ?heard.language, "Sarvam transcribed");
                Heard::sarvam(heard, mode, &d.route, audio_ms)
            }
            Transcript::Byok(heard) => {
                tracing::info!(id, audio_ms, language = ?heard.language, translated = heard.translated, "the speech-to-text service transcribed");
                Heard::byok(heard, &d.route, audio_ms)
            }
        };
        self.got_text(id, heard);
    }

    /// The cloud service didn't answer. The model here takes over when the
    /// user allows it.
    fn cloud_failed(&mut self, id: u64, error: ProviderError) {
        tracing::warn!(id, "couldn't transcribe in the cloud: {error}");
        let privacy = self.state.settings.get().privacy;
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.working = None;
        let local = d.local.is_some() || d.route.local.is_some();
        if !(privacy.fallback_to_local && local) {
            return self.fail(
                id,
                Failure {
                    title: "Couldn't transcribe",
                    message: error.to_string(),
                    account: error.needs_account(),
                    local,
                },
            );
        }
        if privacy.fallback_toast {
            let mut toast = Toast::new(Tone::Neutral, "Transcribed on this PC instead")
                .detail(error.to_string());
            if error.needs_account() {
                toast = toast.action(ToastAction::OpenProviders, "Providers");
            }
            self.toast(toast);
        }
        self.use_local(id);
    }

    /// Carries on with the model here, with what it already heard or by
    /// having it listen to the recording now.
    fn use_local(&mut self, id: u64) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.fell_back = true;
        if let Some(result) = d.local.take() {
            let heard = Heard::local(&result, &d.route);
            return self.got_text(id, heard);
        }
        let Some(local) = d.route.local.clone() else {
            return;
        };
        d.stage = Stage::WaitingForEngine;
        let generation = lock(&d.tap).reopen();
        self.open_engine(id, generation, &local);
    }

    /// A transcript is in. It's translated or romanized when the route asks
    /// for that, then typed. Smart polish does both in one go.
    fn got_text(&mut self, id: u64, heard: Heard) {
        if heard.raw.is_empty() {
            return self.nothing_heard(id, heard.speech);
        }
        if self
            .dictations
            .get(&id)
            .is_some_and(|d| d.mode == Mode::Command)
        {
            return self.command(id, heard);
        }
        let settings = self.state.settings.get();
        let ai = self.ai(&settings);
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let rewrite = d.route.rewrite(
            heard.language.as_deref(),
            heard.output.is_some(),
            heard.romanized,
        );
        let text = heard.raw.clone();
        d.heard = Some(heard);
        let Some(rewrite) = rewrite else {
            return self.finish(id, text, None);
        };
        let ai_translates = settings.polish.translate && ai.is_ok();
        if ai_translates && settings.polish.level == PolishLevel::Smart {
            return self.finish(id, text, Some(rewrite));
        }
        if sarvam_rewrites(&settings, &rewrite) {
            return self.sarvam_rewrite(id, text, rewrite, settings.languages.script);
        }
        match ai {
            Ok(chosen) if ai_translates => {
                self.ai_rewrite(id, text, rewrite, chosen, settings.languages.script)
            }
            _ => {
                self.untranslated(&rewrite, ai.is_ok());
                self.finish(id, text, None);
            }
        }
    }

    // Command Mode

    /// Reads the selected text with a simulated Ctrl+C, once the user lets
    /// go of the shortcut, while the instruction is transcribed.
    fn read_selection(&mut self, id: u64) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.selection = Selection::Reading;
        let state = self.state.clone();
        let session = self.session.clone();
        async_runtime::spawn_blocking(move || {
            let text = state.inserter.as_ref().and_then(|inserter| {
                let inserter = inserter.lock().unwrap_or_else(|e| e.into_inner());
                inserter
                    .copy_selection()
                    .inspect_err(|e| tracing::warn!("couldn't read the selected text: {e:#}"))
                    .ok()
                    .flatten()
            });
            session.send(Command::SelectionRead { id, text });
        });
    }

    fn selection_read(&mut self, id: u64, text: Option<String>) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        tracing::debug!(
            id,
            chars = text.as_ref().map_or(0, |t| t.chars().count()),
            "read the selected text"
        );
        d.selection = Selection::Read(text);
        if d.stage == Stage::Selecting {
            self.run_command(id);
        }
    }

    /// The instruction is in. It runs once the selected text is read.
    fn command(&mut self, id: u64, heard: Heard) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.heard = Some(heard);
        match d.selection {
            Selection::Read(_) => self.run_command(id),
            Selection::Reading => d.stage = Stage::Selecting,
            Selection::Unread => {
                d.stage = Stage::Selecting;
                self.read_selection(id);
            }
        }
    }

    /// Has the AI model carry out the instruction on the selected text.
    fn run_command(&mut self, id: u64) {
        let settings = self.state.settings.get();
        let chosen = match self.ai(&settings) {
            Ok(chosen) => chosen,
            Err(why) => {
                self.dictations.remove(&id);
                return self.command_needs_ai(&why);
            }
        };
        let rules = self.state.rules();
        let category = insert::foreground().map_or(Category::Other, |app| {
            rules.styles.category(&app.exe, &app.title)
        });
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let (Some(heard), Selection::Read(selection)) = (&d.heard, &d.selection) else {
            return;
        };
        let request = ai::Command::new(
            heard.raw.clone(),
            selection.clone(),
            rules.vocabulary.terms().to_vec(),
        );
        tracing::debug!(
            id,
            words = request.words(),
            selected = request.selection.is_some(),
            "running a command"
        );
        d.stage = Stage::Commanding;
        d.working = chosen.cloud_name().map(str::to_owned);
        d.category = Some(category);
        let (keys, local, tx) = (
            self.state.keys.clone(),
            self.state.local_ai.clone(),
            self.session.clone(),
        );
        async_runtime::spawn(async move {
            let result = ai::run(&chosen, &keys, &local, &request).await;
            tx.send(Command::Commanded { id, result });
        });
    }

    /// The AI model answered. Its text replaces the selection, or goes in at
    /// the cursor, as it is.
    fn commanded(&mut self, id: u64, result: Result<String, ProviderError>) {
        if self.stage(id) != Some(Stage::Commanding) {
            return;
        }
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let working = d.working.take();
        let text = match result {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!(id, "the AI model couldn't carry out the command: {e}");
                return self.fail(
                    id,
                    Failure {
                        title: "Couldn't run the command",
                        message: e.to_string(),
                        account: needs_providers_page(&e),
                        local: false,
                    },
                );
            }
        };
        if let (Some(heard), Some(name)) = (d.heard.as_mut(), &working) {
            add_cloud(&mut heard.cloud, name);
        }
        d.stage = Stage::Inserting;
        d.text = text.clone();
        self.insert(id, text, false);
    }

    fn command_needs_ai(&mut self, why: &ai::NotReady) {
        self.cue(Cue::Error);
        self.toast(
            Toast::new(Tone::Warning, "Command Mode needs an AI model")
                .detail(format!("{why} Set one up on the Providers page."))
                .action(ToastAction::OpenProviders, "Providers"),
        );
    }

    // Rewriting

    fn sarvam_rewrite(&mut self, id: u64, text: String, rewrite: Rewrite, script: Script) {
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.stage = Stage::Rewriting;
        d.working = Some(sarvam::NAME.to_owned());
        let keys = self.state.keys.clone();
        let tx = self.session.clone();
        async_runtime::spawn(async move {
            let result = async {
                let sarvam = Sarvam::new(providers::key(&keys, SARVAM).await?);
                match &rewrite {
                    Rewrite::Translate { from, to } => {
                        sarvam.translate(&text, from, to, script).await
                    }
                    Rewrite::Romanize { language } => sarvam.romanize(&text, language).await,
                }
            }
            .await;
            tx.send(Command::Rewritten {
                id,
                rewrite,
                by: Some(sarvam::NAME.to_owned()),
                result,
            });
        });
    }

    /// Has the AI model only translate or romanize, for when Smart polish is off.
    fn ai_rewrite(
        &mut self,
        id: u64,
        text: String,
        rewrite: Rewrite,
        chosen: ai::Chosen,
        script: Script,
    ) {
        let terms = self.state.rules().vocabulary.terms().to_vec();
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.stage = Stage::Rewriting;
        d.working = chosen.cloud_name().map(str::to_owned);
        let by = d.working.clone();
        let request = ai::Request::rewrite(text, rewrite.clone(), script, terms);
        let (keys, local, tx) = (
            self.state.keys.clone(),
            self.state.local_ai.clone(),
            self.session.clone(),
        );
        async_runtime::spawn(async move {
            let result = ai::run(&chosen, &keys, &local, &request).await;
            tx.send(Command::Rewritten {
                id,
                rewrite,
                by,
                result,
            });
        });
    }

    fn rewritten(
        &mut self,
        id: u64,
        rewrite: Rewrite,
        by: Option<String>,
        result: Result<String, ProviderError>,
    ) {
        if self.stage(id) != Some(Stage::Rewriting) {
            return;
        }
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.working = None;
        let Some(heard) = d.heard.as_mut() else {
            return;
        };
        let text = match result {
            Ok(text) => {
                if let Rewrite::Translate { to, .. } = &rewrite {
                    heard.output = Some(to.clone());
                }
                if let Some(by) = &by {
                    add_cloud(&mut heard.cloud, by);
                }
                text
            }
            Err(e) => {
                tracing::warn!(id, "couldn't rewrite the transcript: {e}");
                let text = heard.raw.clone();
                let title = match rewrite {
                    Rewrite::Translate { .. } => "Couldn't translate",
                    Rewrite::Romanize { .. } => "Couldn't write it in English letters",
                };
                let mut toast = Toast::new(Tone::Warning, title)
                    .detail(format!("{e} Joona typed what you said as it was."));
                if needs_providers_page(&e) {
                    toast = toast.action(ToastAction::OpenProviders, "Providers");
                }
                self.toast(toast);
                text
            }
        };
        self.finish(id, text, None);
    }

    /// Says once why text isn't translated or romanized as the route asks.
    /// `ai_ready` says an AI model is set up but not allowed to translate.
    fn untranslated(&mut self, rewrite: &Rewrite, ai_ready: bool) {
        if std::mem::replace(&mut self.told_untranslated, true) {
            return;
        }
        let (title, task, sarvam_can) = match rewrite {
            Rewrite::Translate { from, to } => (
                "Not translated",
                format!(
                    "translate {} into {}",
                    languages::name(from),
                    languages::name(to)
                ),
                sarvam::can_translate(from, to),
            ),
            Rewrite::Romanize { language } => (
                "Not written in English letters",
                format!("write {} in English letters", languages::name(language)),
                sarvam::can_romanize(language),
            ),
        };
        let fix = match (ai_ready, sarvam_can) {
            (true, _) => "Let your AI model translate text on the Providers page",
            (false, true) => "Add Sarvam or an AI model on the Providers page",
            (false, false) => "Add an AI model on the Providers page",
        };
        self.toast(
            Toast::new(Tone::Neutral, title)
                .detail(format!("{fix} to {task}."))
                .action(ToastAction::OpenProviders, "Providers"),
        );
    }

    /// Runs the text pipeline, with the AI model for Smart polish. `folded`
    /// is a translation the AI model does while it polishes.
    fn finish(&mut self, id: u64, raw: String, folded: Option<Rewrite>) {
        let settings = self.state.settings.get();
        let rules = self.state.rules();
        let category = insert::foreground().map_or(Category::Other, |app| {
            rules.styles.category(&app.exe, &app.title)
        });
        let prepared =
            pipeline::prepare(&raw, &rules.vocabulary, settings.experimental.press_enter);
        let mut level = settings.polish.level;
        if level == PolishLevel::Smart && prepared.has_words() {
            match self.ai(&settings) {
                Ok(chosen) => return self.polish(id, prepared, chosen, folded, category),
                Err(why) => self.no_ai(&why),
            }
        }
        if level == PolishLevel::Smart {
            level = PolishLevel::Basic;
        }
        let out = pipeline::complete(
            prepared,
            &rules.vocabulary,
            &pipeline::Options {
                polish: level,
                tone: rules.styles.tone(category),
                press_enter: settings.experimental.press_enter,
                trailing_space: settings.insertion.trailing_space,
            },
        );
        self.deliver(id, out, category);
    }

    /// Has the AI model polish the prepared text.
    fn polish(
        &mut self,
        id: u64,
        prepared: pipeline::Prepared,
        chosen: ai::Chosen,
        folded: Option<Rewrite>,
        category: Category,
    ) {
        let settings = self.state.settings.get();
        let rules = self.state.rules();
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let language = d
            .heard
            .as_ref()
            .and_then(|h| h.output.clone().or_else(|| h.language.clone()));
        let request = ai::Request {
            text: ai::prompt::to_markers(prepared.text()),
            markers: prepared.snippets(),
            polish: true,
            tone: rules.styles.tone(category),
            category,
            language,
            rewrite: folded.clone(),
            script: settings.languages.script,
            terms: rules.vocabulary.terms().to_vec(),
        };
        d.stage = Stage::Polishing;
        d.working = chosen.cloud_name().map(str::to_owned);
        d.prepared = Some(prepared);
        d.folded = folded;
        d.category = Some(category);
        let (keys, local, tx) = (
            self.state.keys.clone(),
            self.state.local_ai.clone(),
            self.session.clone(),
        );
        async_runtime::spawn(async move {
            let result = ai::run(&chosen, &keys, &local, &request).await;
            tx.send(Command::Polished { id, result });
        });
    }

    /// The AI model answered. When it couldn't polish the text, Basic polish does.
    fn polished(&mut self, id: u64, result: Result<String, ProviderError>) {
        if self.stage(id) != Some(Stage::Polishing) {
            return;
        }
        let settings = self.state.settings.get();
        let rules = self.state.rules();
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        let (Some(prepared), Some(category)) = (d.prepared.take(), d.category) else {
            return;
        };
        let working = d.working.take();
        let folded = d.folded.take();
        if result.is_ok()
            && let Some(heard) = d.heard.as_mut()
        {
            if let Some(Rewrite::Translate { to, .. }) = &folded {
                heard.output = Some(to.clone());
            }
            if let Some(name) = &working {
                add_cloud(&mut heard.cloud, name);
            }
        }
        let out = match result {
            Ok(text) => pipeline::complete_polished(
                prepared,
                &ai::prompt::from_markers(&text),
                folded.is_some(),
                &rules.vocabulary,
                settings.insertion.trailing_space,
            ),
            Err(e) => {
                tracing::warn!(id, "the AI model couldn't polish the text: {e}");
                let title = match &folded {
                    Some(Rewrite::Translate { .. }) => "Not translated",
                    Some(Rewrite::Romanize { .. }) => "Not written in English letters",
                    None => "Used Basic polish",
                };
                let mut toast = Toast::new(Tone::Warning, title)
                    .detail(format!("{e} Joona used Basic polish instead."));
                if needs_providers_page(&e) {
                    toast = toast.action(ToastAction::OpenProviders, "Providers");
                }
                self.toast(toast);
                pipeline::complete(
                    prepared,
                    &rules.vocabulary,
                    &pipeline::Options {
                        polish: PolishLevel::Basic,
                        tone: rules.styles.tone(category),
                        press_enter: settings.experimental.press_enter,
                        trailing_space: settings.insertion.trailing_space,
                    },
                )
            }
        };
        self.deliver(id, out, category);
    }

    /// Says once that Smart polish isn't ready, so Basic polish is used.
    fn no_ai(&mut self, why: &ai::NotReady) {
        if std::mem::replace(&mut self.told_no_ai, true) {
            return;
        }
        self.toast(
            Toast::new(Tone::Neutral, "Using Basic polish")
                .detail(format!(
                    "{why} Joona uses Basic polish until Smart polish is set up on the Providers page."
                ))
                .action(ToastAction::OpenProviders, "Providers"),
        );
    }

    fn ai(&self, settings: &Settings) -> Result<ai::Chosen, ai::NotReady> {
        ai::chosen(settings, &self.state.keys, &self.state.local_ai)
    }

    /// Joona's own AI model takes seconds to load, so it loads while the user
    /// speaks when the dictation is likely to need it.
    fn warm_up_ai(&self, settings: &Settings, route: &Route, mode: Mode) {
        let polish = &settings.polish;
        let rewrites = route.output.is_some() || route.script == Script::Roman;
        let needed = mode == Mode::Command
            || polish.level == PolishLevel::Smart
            || (polish.translate && rewrites);
        let local = &self.state.local_ai;
        if !needed
            || polish.provider.as_deref() != Some(providers::llm::LOCAL)
            || local.running()
            || !local.installed()
        {
            return;
        }
        let local = local.clone();
        async_runtime::spawn(async move {
            if let Err(e) = local.start().await {
                tracing::warn!("couldn't start the local AI model: {e:#}");
            }
        });
    }

    /// Types the finished text.
    fn deliver(&mut self, id: u64, out: pipeline::Output, category: Category) {
        if out.text.is_empty() && !out.enter {
            return self.nothing_heard(id, true);
        }
        self.count_uses(&out);
        let Some(d) = self.dictations.get_mut(&id) else {
            return;
        };
        d.stage = Stage::Inserting;
        d.text = out.text.clone();
        d.fixes = out.fixes;
        d.category = Some(category);
        self.insert(id, out.text, out.enter);
    }

    /// Counts the dictionary words and snippets a dictation used, which ranks
    /// them on their pages.
    fn count_uses(&self, out: &pipeline::Output) {
        let Some(db) = self.state.db.clone() else {
            return;
        };
        if out.words.is_empty() && out.snippets.is_empty() {
            return;
        }
        let (words, snippets) = (out.words.clone(), out.snippets.clone());
        async_runtime::spawn_blocking(move || {
            let now = history::now_ms();
            if let Err(e) = db
                .dictionary_used(&words, now)
                .and_then(|()| db.snippets_used(&snippets, now))
            {
                tracing::warn!("couldn't count dictionary and snippet uses: {e:#}");
            }
        });
    }

    fn nothing_heard(&mut self, id: u64, speech: bool) {
        self.dictations.remove(&id);
        let detail = match speech {
            false => "Joona didn't hear any speech.",
            true => "Try again, a little closer to the microphone.",
        };
        self.toast(Toast::new(Tone::Neutral, "Didn't catch that").detail(detail));
    }

    fn fail(&mut self, id: u64, failure: Failure) {
        let Some(d) = self.dictations.remove(&id) else {
            return;
        };
        if self.current == Some(id) {
            self.current = None;
            self.end_gesture();
        }
        self.cue(Cue::Error);
        let samples = std::mem::take(&mut lock(&d.tap).recording).into_samples();
        let retry = samples_to_ms(samples.len()) >= MIN_AUDIO_MS;
        let mut toast = Toast::new(Tone::Error, failure.title).detail(failure.message);
        if failure.account {
            toast = toast.action(ToastAction::OpenProviders, "Providers");
        }
        if retry && failure.local {
            toast = toast.action(ToastAction::UseLocal, "Use this PC");
        }
        if retry && toast.actions.len() < 2 {
            toast = toast.action(ToastAction::Retry, "Retry");
        }
        let toast = self.toast(toast);
        if retry {
            self.keep(toast, samples, d.mode, d.route);
        }
    }

    // Insertion

    /// Inserts `text`, then presses Enter if `enter`. With no text, only
    /// presses Enter.
    fn insert(&self, id: u64, text: String, enter: bool) {
        let state = self.state.clone();
        let session = self.session.clone();
        async_runtime::spawn_blocking(move || {
            let app = insert::foreground().map(|t| t.exe);
            let options = state.settings.get().insertion.options_for(app.as_deref());
            let result = match &state.inserter {
                Some(inserter) => {
                    let inserter = inserter.lock().unwrap_or_else(|e| e.into_inner());
                    let inserted = match text.is_empty() {
                        true => inserter.press_enter().map(|()| Outcome::Typed),
                        false => inserter.insert(&text, &options).inspect(|outcome| {
                            if enter && matches!(outcome, Outcome::Pasted | Outcome::Typed) {
                                std::thread::sleep(ENTER_DELAY);
                                if let Err(e) = inserter.press_enter() {
                                    tracing::warn!("couldn't press Enter: {e:#}");
                                }
                            }
                        }),
                    };
                    inserted.map_err(|e| InsertFailure {
                        message: format!("{e:#}"),
                        copied: !text.is_empty() && inserter.copy(&text).is_ok(),
                    })
                }
                None => Err(InsertFailure {
                    message: "Joona couldn't start its clipboard helper.".into(),
                    copied: false,
                }),
            };
            session.send(Command::Inserted { id, app, result });
        });
    }

    fn inserted(&mut self, id: u64, app: Option<String>, result: Result<Outcome, InsertFailure>) {
        if let Some(d) = self.dictations.get(&id)
            && !d.text.is_empty()
        {
            self.last_text = Some(d.text.clone());
        }
        let delivery = match &result {
            Ok(Outcome::Pasted) => Delivery::Pasted,
            Ok(Outcome::Typed) => Delivery::Typed,
            Ok(Outcome::Copied(_)) => Delivery::Copied,
            Err(failure) if failure.copied => Delivery::Copied,
            Err(_) => Delivery::Failed,
        };
        self.record(id, app.clone(), delivery);
        match result {
            Ok(outcome @ (Outcome::Pasted | Outcome::Typed)) => {
                tracing::info!(
                    id,
                    app = app.as_deref().unwrap_or("?"),
                    ?outcome,
                    "inserted"
                );
                if let Some(d) = self.dictations.get_mut(&id) {
                    d.stage = Stage::Done(outcome);
                    self.session.send_after(DONE, Command::DoneShown { id });
                }
            }
            Ok(Outcome::Copied(reason)) => {
                tracing::info!(
                    id,
                    app = app.as_deref().unwrap_or("?"),
                    ?reason,
                    "left on the clipboard"
                );
                self.dictations.remove(&id);
                self.copied(reason, app);
            }
            Err(failure) => {
                tracing::warn!(
                    id,
                    app = app.as_deref().unwrap_or("?"),
                    "insertion failed: {}",
                    failure.message
                );
                self.dictations.remove(&id);
                self.cue(Cue::Error);
                let title = match failure.copied {
                    true => "Copied, press Ctrl+V",
                    false => "Couldn't insert the text",
                };
                self.toast(Toast::new(Tone::Error, title).detail(failure.message));
            }
        }
    }

    /// Adds a finished dictation to the history, with its recording when the
    /// user keeps those.
    fn record(&self, id: u64, app: Option<String>, delivery: Delivery) {
        let Some(history) = self.state.history.clone() else {
            return;
        };
        let Some(d) = self.dictations.get(&id) else {
            return;
        };
        let Some(heard) = &d.heard else {
            return;
        };
        let privacy = self.state.settings.get().privacy;
        if !privacy.keep_history {
            return;
        }
        let text = d.text.trim_end().to_owned();
        if text.is_empty() {
            return;
        }
        let entry = Entry {
            id: 0,
            created_at: history::now_ms(),
            app_name: app.as_deref().map(app_name),
            app,
            raw: heard.raw.clone(),
            words: history::count_words(&text),
            text,
            language: heard.language.clone(),
            output: heard.output.clone(),
            audio_ms: heard.audio_ms,
            model: heard.model.clone(),
            cloud: heard.cloud.clone(),
            mode: d.mode,
            delivery,
            fixes: d.fixes,
            category: d.category.map(|c| c.as_str().to_owned()),
            audio: false,
        };
        let samples = privacy
            .save_audio
            .then(|| lock(&d.tap).recording.samples().to_vec());
        let ui = self.state.ui.clone();
        async_runtime::spawn_blocking(move || {
            let mut entry = match history.add(entry) {
                Ok(entry) => entry,
                Err(e) => return tracing::warn!("couldn't save the dictation to history: {e:#}"),
            };
            if let Some(samples) = samples {
                let path = history.audio_path(entry.id);
                let saved = path
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&path, encode_wav(&samples)));
                match saved
                    .map_err(anyhow::Error::from)
                    .and_then(|()| history.set_audio(entry.id, true))
                {
                    Ok(()) => entry.audio = true,
                    Err(e) => tracing::warn!("couldn't keep the recording: {e:#}"),
                }
            }
            ui.history(&entry);
        });
    }

    fn copied(&mut self, reason: CopyReason, app: Option<String>) {
        let name = app.as_deref().map(app_name);
        let detail = match reason {
            CopyReason::Elevated => format!(
                "{} runs as administrator, so Windows won't let Joona type into it.",
                name.as_deref().unwrap_or("This app")
            ),
            CopyReason::NoTarget => "Click into a text field first, then dictate.".into(),
            CopyReason::Unresponsive => {
                format!("{} isn't responding.", name.as_deref().unwrap_or("The app"))
            }
            CopyReason::KeysHeld => "Some keys were still held down.".into(),
            CopyReason::NotAccepted => format!(
                "{} didn't accept the paste.",
                name.as_deref().unwrap_or("The app")
            ),
        };
        let mut toast = Toast::new(Tone::Neutral, "Copied, press Ctrl+V")
            .detail(detail)
            .timeout(6_000);
        let offer = match (reason, app, name) {
            (CopyReason::NotAccepted, Some(exe), Some(name)) => {
                toast = toast.action(ToastAction::AlwaysType, format!("Always type in {name}"));
                Some(exe)
            }
            _ => None,
        };
        let toast = self.toast(toast);
        self.always_type = offer.map(|exe| (toast, exe));
    }

    fn paste_last(&mut self) {
        let Some(text) = self.last_text.clone() else {
            self.toast(
                Toast::new(Tone::Neutral, "Nothing to paste yet")
                    .detail("Your last dictation will be here."),
            );
            return;
        };
        self.insert(PASTE_LAST, text, false);
    }

    // Toasts

    fn toast_action(&mut self, toast: u32, action: ToastAction) {
        match action {
            ToastAction::Undo | ToastAction::Retry | ToastAction::UseLocal => {
                self.state.ui.dismiss_toast(toast);
                match self.kept.take() {
                    Some(kept) if kept.toast == toast => {
                        self.retranscribe(kept, action == ToastAction::UseLocal)
                    }
                    other => self.kept = other,
                }
            }
            ToastAction::AlwaysType => {
                self.state.ui.dismiss_toast(toast);
                let Some((_, exe)) = self.always_type.take_if(|(id, _)| *id == toast) else {
                    return;
                };
                let name = app_name(&exe);
                let saved = self.state.settings.update(|s| {
                    s.insertion
                        .overrides
                        .retain(|o| !o.exe.eq_ignore_ascii_case(&exe));
                    s.insertion.overrides.push(AppOverride {
                        exe,
                        method: Some(Method::Type),
                        paste_timeout_ms: None,
                        restore_delay_ms: None,
                    });
                });
                match saved {
                    Ok(_) => self.toast(Toast::new(
                        Tone::Success,
                        format!("Joona will type into {name} from now on"),
                    )),
                    Err(e) => self.toast(
                        Toast::new(Tone::Error, "Couldn't save that").detail(format!("{e:#}")),
                    ),
                };
            }
            ToastAction::OpenModels => self.state.ui.open_hub(Some("models")),
            ToastAction::OpenLanguages => self.state.ui.open_hub(Some("languages")),
            ToastAction::OpenProviders => self.state.ui.open_hub(Some("providers")),
            ToastAction::OpenMicSettings if audio::mic_access().blocked() => {
                self.state.ui.open_mic_privacy()
            }
            ToastAction::OpenMicSettings => self.state.ui.open_hub(Some("settings/general")),
        }
    }

    fn discarded(&mut self, id: u64) {
        let Some(d) = self.dictations.remove(&id) else {
            return;
        };
        let samples = std::mem::take(&mut lock(&d.tap).recording).into_samples();
        let undo = samples_to_ms(samples.len()) >= MIN_AUDIO_MS;
        let mut toast = Toast::new(Tone::Neutral, "Cancelled").timeout(UNDO_TOAST_MS);
        if undo {
            toast = toast.action(ToastAction::Undo, "Undo");
        }
        let toast = self.toast(toast);
        if undo {
            self.keep(toast, samples, d.mode, d.route);
        }
    }

    fn keep(&mut self, toast: u32, samples: Vec<f32>, mode: Mode, route: Route) {
        self.kept = Some(Kept {
            toast,
            samples,
            mode,
            route,
        });
        self.session
            .send_after(KEEP_AUDIO, Command::ForgetAudio { toast });
    }

    fn forget_kept(&mut self) {
        if let Some(kept) = self.kept.take() {
            self.state.ui.dismiss_toast(kept.toast);
        }
    }

    /// Transcribes kept audio again, on this PC only with `local_only`.
    fn retranscribe(&mut self, kept: Kept, local_only: bool) {
        let listener = match local_only {
            true => kept.route.local.clone(),
            false => listener(&kept.route).cloned(),
        };
        let handoff = match listener {
            Some(_) => None,
            None => kept.route.handoff(None),
        };
        if listener.is_none() && handoff.is_none() {
            return;
        }
        self.next_id += 1;
        let id = self.next_id;
        let mut recording = Recording::new();
        recording.push(&kept.samples);
        let engine = match listener {
            Some(_) => Engine::Opening,
            None => Engine::Closed,
        };
        if let Some(local) = &listener {
            self.open_engine(id, 0, local);
        }
        let mode = kept.mode;
        let tap = Tap::new(recording, engine);
        let mut dictation = Dictation::new(mode, kept.route, tap, Stage::WaitingForEngine);
        dictation.armed = true;
        dictation.fell_back = local_only;
        self.dictations.insert(id, dictation);
        if mode == Mode::Command {
            self.read_selection(id);
        }
        if let Some(handoff) = handoff {
            self.ask_cloud(id, handoff);
        }
    }

    fn route_failed(&mut self, error: RouteError) {
        self.cue(Cue::Error);
        let toast = match error {
            RouteError::NoModel => Toast::new(Tone::Warning, "No speech model yet")
                .detail("Download one to start dictating. It runs on this PC.")
                .action(ToastAction::OpenModels, "Get a model"),
            RouteError::Unsupported(_) => Toast::new(Tone::Warning, error.to_string())
                .detail("Download a multilingual model, or change the languages you speak.")
                .action(ToastAction::OpenModels, "Models")
                .action(ToastAction::OpenLanguages, "Languages"),
        };
        self.toast(toast);
    }

    fn toast(&mut self, mut toast: Toast) -> u32 {
        self.next_toast += 1;
        toast.id = self.next_toast;
        self.state.ui.toast(&toast);
        toast.id
    }

    // Helpers

    fn stage(&self, id: u64) -> Option<Stage> {
        self.dictations.get(&id).map(|d| d.stage)
    }

    fn cue(&self, cue: Cue) {
        if self.state.settings.get().general.sounds {
            self.state.ui.sound(cue);
        }
    }

    fn with_hotkeys(&self, f: impl FnOnce(&crate::hotkeys::Hotkeys)) {
        if let Some(hotkeys) = self
            .state
            .hotkeys
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            f(hotkeys);
        }
    }

    /// Lets the gesture machine know the dictation is over, however it ended.
    fn end_gesture(&self) {
        self.with_hotkeys(|h| h.session_ended());
    }

    fn refresh_bar(&mut self) {
        let state = self.bar_state();
        if without_time(&state) != without_time(&self.bar) {
            self.state.ui.bar(&state);
            *self.state.bar.lock().unwrap_or_else(|e| e.into_inner()) = state.clone();
            self.bar = state;
        }
    }

    fn bar_state(&self) -> BarState {
        if let Some(d) = self.current.and_then(|id| self.dictations.get(&id))
            && d.armed
        {
            return BarState::Listening(Listening {
                mode: d.mode,
                hands_free: d.hands_free,
                elapsed_ms: d.started.elapsed().as_millis().min(u32::MAX as u128) as u32,
                route: d.route.label.clone(),
                ending_soon: d.ending_soon,
            });
        }
        let working = self
            .dictations
            .values()
            .filter(|d| {
                d.armed
                    && matches!(
                        d.stage,
                        Stage::Stopping
                            | Stage::WaitingForEngine
                            | Stage::Transcribing
                            | Stage::Cloud
                            | Stage::Rewriting
                            | Stage::Polishing
                            | Stage::Selecting
                            | Stage::Commanding
                            | Stage::Inserting
                    )
            })
            .max_by_key(|d| d.started);
        if let Some(d) = working {
            return BarState::Processing(Processing {
                route: processing_label(d),
            });
        }
        let done = self.dictations.values().find_map(|d| match d.stage {
            Stage::Done(outcome) => Some(outcome),
            _ => None,
        });
        match done {
            Some(outcome) => BarState::Done(Done { outcome }),
            None => BarState::Resting,
        }
    }
}

fn without_time(state: &BarState) -> BarState {
    let mut state = state.clone();
    if let BarState::Listening(listening) = &mut state {
        listening.elapsed_ms = 0;
    }
    state
}

/// The model that listens while the user speaks, unless a service hears
/// everything this route records.
fn listener(route: &Route) -> Option<&Local> {
    route.local.as_ref().filter(|_| route.listens_locally())
}

/// The route's label, with the cloud badge on while a cloud service works.
fn processing_label(d: &Dictation) -> RouteLabel {
    let mut label = d.route.label.clone();
    label.cloud = match d.stage {
        Stage::Cloud | Stage::Rewriting | Stage::Polishing | Stage::Commanding => d.working.clone(),
        _ if d.fell_back || d.route.listens_locally() => None,
        _ => label.cloud,
    };
    label
}

/// Adds `name` to the cloud services a dictation went to.
fn add_cloud(cloud: &mut Option<String>, name: &str) {
    match cloud {
        Some(names) if names.split(", ").any(|n| n == name) => {}
        Some(names) => {
            names.push_str(", ");
            names.push_str(name);
        }
        None => *cloud = Some(name.to_owned()),
    }
}

/// The fix for `error` is on the Providers page.
fn needs_providers_page(error: &ProviderError) -> bool {
    error.needs_account() || matches!(error, ProviderError::NotRunning(_))
}

/// Whether Sarvam can do `rewrite`. Text goes there only with consent.
fn sarvam_rewrites(settings: &Settings, rewrite: &Rewrite) -> bool {
    settings.privacy.consented_to(SARVAM)
        && match rewrite {
            Rewrite::Translate { from, to } => sarvam::can_translate(from, to),
            Rewrite::Romanize { language } => sarvam::can_romanize(language),
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_states_compare_without_the_timer() {
        let listening = |elapsed_ms| {
            BarState::Listening(Listening {
                mode: Mode::Dictate,
                hands_free: false,
                elapsed_ms,
                route: crate::router::RouteLabel {
                    text: "EN".into(),
                    cross_language: false,
                    cloud: None,
                },
                ending_soon: false,
            })
        };
        assert_eq!(without_time(&listening(10)), without_time(&listening(900)));
        assert_ne!(without_time(&listening(10)), BarState::Resting);
    }
}
