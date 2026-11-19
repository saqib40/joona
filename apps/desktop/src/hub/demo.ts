import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import {
  type AiOverview_Serialize as AiOverview,
  type AiProvider_Serialize as AiProvider,
  type AiSample,
  events,
  type AppInfo_Serialize as AppInfo,
  type AppRule,
  type Category,
  type DayActivity,
  type DictionaryEntry_Serialize as DictionaryEntry,
  type Entry_Serialize as Entry,
  type HomeStats,
  type Insights_Serialize as Insights,
  type Language,
  type LanguageRoutes_Serialize as LanguageRoutes,
  type LocalQuality,
  type MicAccess,
  type ModelCard,
  type ModelsOverview_Serialize as ModelsOverview,
  type Progress_Serialize as Progress,
  type RecentApp,
  type RoutePreview_Serialize as RoutePreview,
  type Settings_Serialize as Settings,
  type Shortcut_Serialize as Shortcut,
  type Snippet_Serialize as Snippet,
  type SttOverview_Serialize as SttOverview,
  type SttProvider_Serialize as SttProvider,
  type StyleOverview,
  type StyleTone,
  type UpdateStatus_Serialize as UpdateStatus,
} from "@/lib/bindings";
import { isLocalUrl, LOCAL_AI, LOCAL_AI_DOWNLOAD } from "./ai";
import { shortcutLabel } from "./shortcut";
import { useHub } from "./store";

/**
 * A pretend backend for designing the Hub in a browser.
 *
 * - `?page=models` or `?page=settings/privacy` opens a page.
 * - `?theme=dark` picks the theme.
 * - `?empty` has no history and no models.
 * - `?onboarding` starts like a fresh install, in onboarding.
 * - `?micblocked` has Windows block the microphone until "Open Windows Settings".
 * - `?update` has a newer version waiting on GitHub.
 *
 * Holding F9 pretends to dictate into the focused text box.
 */
export function installDemo() {
  const params = new URLSearchParams(window.location.search);
  const onboarding = params.has("onboarding");
  const backend = new Backend(params.has("empty") || onboarding);
  if (onboarding) backend.settings = defaultSettings();
  if (params.has("micblocked")) backend.micAccess = "blockedForDesktopApps";
  if (params.has("update")) {
    backend.newer = { state: "available", version: "0.2.0", notes: "Command Mode works in more apps, and Marathi joins Sarvam." };
    backend.update = backend.newer;
  }
  const theme = params.get("theme");
  if (theme === "light" || theme === "dark") backend.settings.general.theme = theme;

  mockWindows("hub");
  mockIPC((cmd, args) => backend.handle(cmd, (args ?? {}) as Record<string, unknown>), { shouldMockEvents: true });
  pretendToDictate(backend);

  const page = params.get("page");
  if (page) useHub.getState().navigate(page);
}

const SAID = [
  "Hi, I'm trying Joona for the first time.",
  "Let's meet on Friday at four and go through the launch plan.",
  "Thanks for the update, this looks great.",
];

/** Hold F9 to talk, let go to "type" into whatever has focus. */
function pretendToDictate(backend: Backend) {
  const route = { text: "EN", crossLanguage: false, cloud: null };
  let held = false;
  let said = 0;
  window.addEventListener("keydown", (e) => {
    if (e.key !== "F9" || held) return;
    e.preventDefault();
    held = true;
    void events.barStateChanged.emit({
      state: "listening",
      mode: "dictate",
      handsFree: false,
      elapsedMs: 0,
      route,
      endingSoon: false,
    });
  });
  window.addEventListener("keyup", (e) => {
    if (e.key !== "F9" || !held) return;
    held = false;
    void events.barStateChanged.emit({ state: "processing", route });
    window.setTimeout(() => {
      if (backend.installed.size === 0) {
        void events.barStateChanged.emit({ state: "resting" });
        void events.toastShown.emit({
          id: Date.now(),
          tone: "error",
          title: "No speech model yet",
          detail: "Download one to start dictating.",
          actions: [{ action: "openModels", label: "Models" }],
          timeoutMs: 6000,
        });
        return;
      }
      const text = SAID[said++ % SAID.length];
      // execCommand fires the same input event a real paste does.
      document.execCommand("insertText", false, `${text} `);
      void events.barStateChanged.emit({ state: "done", outcome: { kind: "pasted" } });
      const entry = backend.add(text);
      void events.historyAdded.emit(entry);
      window.setTimeout(() => void events.barStateChanged.emit({ state: "resting" }), 900);
    }, 700);
  });
}

const DAY_MS = 86_400_000;
const ENGLISH_AND_EUROPE = "bg hr cs da nl en et fi fr de el hu it lv lt mt pl pt ro sk sl es sv ru uk".split(" ");

const CATALOG: ModelCard[] = [
  model("parakeet-tdt-0.6b-v3", "Parakeet v3", "parakeet", "Fast and very accurate for English and 24 other European languages.", {
    languages: ENGLISH_AND_EUROPE,
    speed: 4,
    accuracy: 4,
    size: 670_619_803,
    ramMb: 1500,
    license: "CC-BY-4.0",
    credit: "NVIDIA Parakeet TDT 0.6B v3, ONNX export by Ilya Stupakov",
  }),
  model("moonshine-base", "Moonshine Base", "moonshine", "Small and quick, English only. Good for older PCs.", {
    languages: ["en"],
    multilingual: false,
    speed: 5,
    accuracy: 2,
    size: 77_216_803,
    ramMb: 400,
    credit: "Moonshine by Useful Sensors",
  }),
  model("whisper-tiny", "Whisper Tiny", "whisper", "The smallest and fastest Whisper, in 99 languages. Rough accuracy.", {
    translate: true,
    speed: 5,
    accuracy: 1,
    size: 77_691_713,
    ramMb: 400,
  }),
  model("whisper-base", "Whisper Base", "whisper", "Fast, with fair accuracy in 99 languages. Translates into English.", {
    translate: true,
    speed: 5,
    accuracy: 2,
    size: 147_951_465,
    ramMb: 500,
  }),
  model("whisper-small", "Whisper Small", "whisper", "A good balance of speed and accuracy in 99 languages. Translates into English.", {
    translate: true,
    speed: 3,
    accuracy: 3,
    size: 487_601_967,
    ramMb: 1000,
  }),
  model("whisper-large-v3-turbo-q5_0", "Whisper Large v3 Turbo", "whisper", "Accurate in 99 languages and quick on most PCs. Can't translate.", {
    speed: 3,
    accuracy: 4,
    size: 574_041_195,
    ramMb: 1200,
  }),
  model("whisper-large-v3-q5_0", "Whisper Large v3", "whisper", "The most accurate Whisper, and the best for translating into English. Needs a fast PC.", {
    translate: true,
    speed: 1,
    accuracy: 5,
    size: 1_081_140_203,
    ramMb: 2000,
  }),
];

function model(
  id: string,
  name: string,
  engine: ModelCard["engine"],
  summary: string,
  extra: Partial<ModelCard> & { size: number },
): ModelCard {
  return {
    id,
    name,
    engine,
    summary,
    languages: [],
    multilingual: true,
    translate: false,
    speed: 3,
    accuracy: 3,
    downloadSize: Math.round(extra.size * 0.98),
    ramMb: 500,
    license: "MIT",
    credit: "OpenAI Whisper, converted to GGML for whisper.cpp",
    installed: false,
    custom: false,
    fit: "good",
    ...extra,
  };
}

const lang = (code: string, name: string, native: string, local: LocalQuality, sarvam = false, latin = false): Language => ({
  code,
  name,
  native,
  whisper: local !== "none",
  local,
  sarvam,
  latin,
});

/** Part of `languages.rs`: English and the Indian languages, then a few others. */
const LANGUAGES: Language[] = [
  lang("en", "English", "English", "good", true, true),
  lang("hi", "Hindi", "हिन्दी", "fair", true),
  lang("bn", "Bengali", "বাংলা", "weak", true),
  lang("ta", "Tamil", "தமிழ்", "fair", true),
  lang("te", "Telugu", "తెలుగు", "weak", true),
  lang("mr", "Marathi", "मराठी", "weak", true),
  lang("gu", "Gujarati", "ગુજરાતી", "weak", true),
  lang("kn", "Kannada", "ಕನ್ನಡ", "weak", true),
  lang("ml", "Malayalam", "മലയാളം", "weak", true),
  lang("pa", "Punjabi", "ਪੰਜਾਬੀ", "weak", true),
  lang("or", "Odia", "ଓଡ଼ିଆ", "none", true),
  lang("as", "Assamese", "অসমীয়া", "weak", true),
  lang("ur", "Urdu", "اردو", "fair", true),
  lang("ne", "Nepali", "नेपाली", "weak", true),
  lang("kok", "Konkani", "कोंकणी", "none", true),
  lang("ar", "Arabic", "العربية", "fair"),
  lang("zh", "Chinese", "中文", "good"),
  lang("fr", "French", "Français", "good", false, true),
  lang("de", "German", "Deutsch", "good", false, true),
  lang("ja", "Japanese", "日本語", "good"),
  lang("ko", "Korean", "한국어", "good"),
  lang("pt", "Portuguese", "Português", "good", false, true),
  lang("ru", "Russian", "Русский", "good"),
  lang("es", "Spanish", "Español", "good", false, true),
];

const MICROPHONES = [
  { id: "{0.0.1.00000000}.{6b1a}", name: "Microphone Array (Realtek Audio)", isDefault: true },
  { id: "{0.0.1.00000000}.{9c2f}", name: "Headset (Jabra Evolve2 65)", isDefault: false },
];

const provider = (p: Partial<AiProvider> & Pick<AiProvider, "id" | "name" | "summary">): AiProvider => ({
  key: "required",
  local: false,
  endpoint: false,
  defaultEndpoint: null,
  models: [],
  keysUrl: null,
  privacyUrl: null,
  note: null,
  ...p,
});

/** A copy of `llm::SPECS`. */
const AI_PROVIDERS: AiProvider[] = [
  provider({
    id: "openai",
    name: "OpenAI",
    summary: "GPT models. Quick, and good at following your style.",
    defaultEndpoint: "https://api.openai.com/v1",
    models: ["gpt-4.1-mini", "gpt-4.1-nano", "gpt-4.1", "gpt-5-mini"],
    keysUrl: "https://platform.openai.com/api-keys",
    privacyUrl: "https://openai.com/policies/privacy-policy/",
  }),
  provider({
    id: "anthropic",
    name: "Anthropic",
    summary: "Claude models. Careful with meaning and tone.",
    defaultEndpoint: "https://api.anthropic.com/v1",
    models: ["claude-haiku-4-5", "claude-sonnet-4-5"],
    keysUrl: "https://console.anthropic.com/settings/keys",
    privacyUrl: "https://www.anthropic.com/legal/privacy",
  }),
  provider({
    id: "gemini",
    name: "Google Gemini",
    summary: "Gemini models, with a free tier.",
    defaultEndpoint: "https://generativelanguage.googleapis.com/v1beta/openai",
    models: ["gemini-2.5-flash-lite", "gemini-2.5-flash"],
    keysUrl: "https://aistudio.google.com/apikey",
    privacyUrl: "https://ai.google.dev/gemini-api/terms",
    note: "On the free tier, Google may use what you send to improve its products.",
  }),
  provider({
    id: "groq",
    name: "Groq",
    summary: "Open models on very fast chips, with a free tier.",
    defaultEndpoint: "https://api.groq.com/openai/v1",
    models: ["llama-3.3-70b-versatile", "llama-3.1-8b-instant"],
    keysUrl: "https://console.groq.com/keys",
    privacyUrl: "https://groq.com/privacy-policy/",
  }),
  provider({
    id: "openrouter",
    name: "OpenRouter",
    summary: "One key for hundreds of models from every lab.",
    defaultEndpoint: "https://openrouter.ai/api/v1",
    models: ["openai/gpt-4.1-mini", "anthropic/claude-haiku-4.5", "google/gemini-2.5-flash-lite"],
    keysUrl: "https://openrouter.ai/settings/keys",
    privacyUrl: "https://openrouter.ai/privacy",
    note: "OpenRouter passes your text on to the company behind the model you pick.",
  }),
  provider({
    id: "sarvam",
    name: "Sarvam",
    summary: "Sarvam-M, made for Indian languages. Uses your Sarvam key.",
    defaultEndpoint: "https://api.sarvam.ai/v1",
    models: ["sarvam-m"],
    keysUrl: "https://dashboard.sarvam.ai/key-management",
    privacyUrl: "https://www.sarvam.ai/privacy-policy",
  }),
  provider({
    id: LOCAL_AI,
    name: "Local AI",
    summary: "A free model that Joona runs on this PC. No account needed.",
    key: "never",
    local: true,
    models: ["qwen3-4b-instruct"],
  }),
  provider({
    id: "ollama",
    name: "Ollama",
    summary: "Models you run yourself with Ollama.",
    key: "never",
    local: true,
    endpoint: true,
    defaultEndpoint: "http://localhost:11434/v1",
  }),
  provider({
    id: "lmstudio",
    name: "LM Studio",
    summary: "Models you run yourself with LM Studio.",
    key: "optional",
    local: true,
    endpoint: true,
    defaultEndpoint: "http://localhost:1234/v1",
  }),
  provider({
    id: "custom",
    name: "Custom server",
    summary: "Any server with an OpenAI-compatible API, such as llama.cpp or vLLM.",
    key: "optional",
    endpoint: true,
  }),
];

const service = (p: Partial<SttProvider> & Pick<SttProvider, "id" | "name" | "summary">): SttProvider => ({
  key: "required",
  endpoint: false,
  models: [],
  translates: [],
  terms: true,
  keysUrl: null,
  privacyUrl: null,
  note: null,
  ...p,
});

/** A copy of `stt::SPECS`. */
const STT_PROVIDERS: SttProvider[] = [
  service({
    id: "openai",
    name: "OpenAI",
    summary: "GPT Transcribe. Accurate in many languages, and good with names.",
    models: ["gpt-transcribe", "gpt-4o-mini-transcribe"],
    keysUrl: "https://platform.openai.com/api-keys",
    privacyUrl: "https://openai.com/policies/privacy-policy/",
  }),
  service({
    id: "groq",
    name: "Groq",
    summary: "Whisper Large v3 on very fast chips, with a free tier.",
    models: ["whisper-large-v3-turbo", "whisper-large-v3"],
    translates: ["whisper-large-v3"],
    keysUrl: "https://console.groq.com/keys",
    privacyUrl: "https://groq.com/privacy-policy/",
  }),
  service({
    id: "deepgram",
    name: "Deepgram",
    summary: "Nova-3. Quick, and listens for the words in your dictionary.",
    models: ["nova-3", "nova-2"],
    keysUrl: "https://console.deepgram.com/",
    privacyUrl: "https://deepgram.com/privacy",
  }),
  service({
    id: "elevenlabs",
    name: "ElevenLabs",
    summary: "Scribe. Strong in more than 90 languages, Indian ones included.",
    models: ["scribe_v2"],
    terms: false,
    keysUrl: "https://elevenlabs.io/app/settings/api-keys",
    privacyUrl: "https://elevenlabs.io/privacy-policy",
  }),
  service({
    id: "custom-stt",
    name: "Your own server",
    summary: "A Whisper server with an OpenAI-compatible API, such as Speaches or LocalAI.",
    key: "optional",
    endpoint: true,
  }),
];

const LOCAL_AI_INFO = {
  name: "Qwen3 4B Instruct",
  size: 2_522_836_330,
  ramMb: 3400,
  license: "Apache-2.0",
  credit: "Qwen3 by the Qwen team at Alibaba Cloud, quantized by bartowski, run with llama.cpp (MIT)",
};

const AI_SAMPLE: AiSample = {
  before:
    "um so I think we should uh meet on Tuesday no wait Wednesday at 3 and bring the slides the budget and the timeline",
  after: "I think we should meet on Wednesday at 3 and bring the slides, the budget, and the timeline.",
  ms: 0,
};

const wait = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms));

class Backend {
  settings: Settings;
  entries: Entry[];
  installed: Set<string>;
  activity: DayActivity[];
  downloads = new Map<string, { progress: Progress; timer: number }>();
  micTimer = 0;
  recordTimers: number[] = [];
  micAccess: MicAccess = "allowed";
  /** Providers with a saved key. */
  keys = new Set<string>();
  localAi = false;
  dictionary: DictionaryEntry[];
  snippets: Snippet[];
  tones = new Map<Category, StyleTone>();
  /** The user's category choices, by program or website. */
  chosen = new Map<string, Category>();
  update: UpdateStatus = { state: "unknown" };
  /** What checking for updates finds. */
  newer: UpdateStatus | null = null;

  constructor(empty: boolean) {
    this.settings = defaultSettings();
    this.settings.onboarded = true;
    if (!empty) {
      this.settings.languages.speak = [
        { code: "en", engine: "local" },
        { code: "hi", engine: "local" },
        { code: "ta", engine: "sarvam" },
      ];
      this.settings.languages.output = "en";
      this.settings.privacy.consented = ["sarvam"];
      this.keys.add("sarvam");
    }
    this.entries = empty ? [] : sampleEntries(Date.now());
    this.installed = new Set(empty ? [] : ["parakeet-tdt-0.6b-v3", "whisper-base"]);
    this.activity = empty ? [] : sampleActivity(Date.now());
    this.dictionary = empty ? [] : sampleDictionary(Date.now());
    this.snippets = empty ? [] : sampleSnippets(Date.now());
    if (!empty) this.chosen.set("Notion", "work");
  }

  handle(cmd: string, args: Record<string, unknown>): unknown {
    switch (cmd) {
      case "get_settings":
        return this.settings;
      case "set_settings": {
        const settings = structuredClone(args.settings as Settings);
        if (!settings.privacy.consented.includes("sarvam")) {
          for (const choice of settings.languages.speak) choice.engine = "local";
        }
        this.settings = settings;
        return this.settings;
      }
      case "set_paused":
        this.settings.general.paused = args.paused as boolean;
        void events.settingsChanged.emit(this.settings);
        return null;
      case "list_microphones":
        return MICROPHONES;
      case "app_info":
        return APP_INFO;
      case "update_status":
        return this.update;
      case "update_check":
        return this.checkForUpdate();
      case "update_install":
        this.installUpdate();
        return null;
      case "models_overview":
        return this.overview();
      case "model_download":
        this.download(args.id as string);
        return null;
      case "model_cancel":
        this.cancel(args.id as string);
        return null;
      case "model_delete":
        this.installed.delete(args.id as string);
        void events.modelsChanged.emit();
        return null;
      case "model_use":
        if (args.english) this.settings.models.english = (args.id as string | null) ?? null;
        else this.settings.models.active = (args.id as string | null) ?? null;
        void events.settingsChanged.emit(this.settings);
        void events.modelsChanged.emit();
        return null;
      case "history_list":
        return this.list(args.search as string, args.before as number | null, args.limit as number);
      case "history_delete":
        this.entries = this.entries.filter((e) => e.id !== args.id);
        void events.historyChanged.emit();
        return null;
      case "history_clear":
        this.entries = [];
        this.activity = [];
        void events.historyChanged.emit();
        return null;
      case "history_export":
        return this.entries.length;
      case "home_stats":
        return this.homeStats();
      case "insights":
        return this.insights();
      case "shortcut_record":
        this.record();
        return null;
      case "shortcut_record_cancel":
        this.stopRecording();
        return null;
      case "mic_test_start":
        this.startMicTest();
        return MICROPHONES.find((m) => m.id === args.device)?.name ?? MICROPHONES[0].name;
      case "mic_test_stop":
        window.clearInterval(this.micTimer);
        return null;
      case "mic_access":
        return this.micAccess;
      case "open_mic_privacy":
        this.micAccess = "allowed";
        window.setTimeout(() => window.dispatchEvent(new Event("focus")), 1200);
        return null;
      case "languages":
        return LANGUAGES;
      case "language_routes":
        return this.routes(args.settings as Settings);
      case "provider_keys":
        return [...this.keys];
      case "provider_key_set":
        this.keys.add(args.provider as string);
        return null;
      case "provider_key_delete":
        this.keys.delete(args.provider as string);
        return null;
      case "provider_test":
        return this.testKey(args.provider as string, args.key as string | null);
      case "stt_overview":
        return this.sttOverview();
      case "ai_overview":
        return this.aiOverview();
      case "ai_models":
        return this.aiModels(args.provider as string, args.endpoint as string | null);
      case "ai_test":
        return this.aiTest();
      case "local_ai_download":
        this.download(LOCAL_AI_DOWNLOAD);
        return null;
      case "local_ai_cancel":
        this.cancel(LOCAL_AI_DOWNLOAD);
        return null;
      case "local_ai_delete":
        this.localAi = false;
        if (this.settings.polish.provider === LOCAL_AI) {
          this.settings.polish.provider = null;
          this.settings.polish.model = null;
          void events.settingsChanged.emit(this.settings);
        }
        void events.aiChanged.emit();
        return null;
      case "dictionary_list":
        return this.sortedDictionary();
      case "dictionary_add":
        return this.saveWord(null, args.word as string, args.heard as string | null);
      case "dictionary_update":
        return this.saveWord(args.id as number, args.word as string, args.heard as string | null);
      case "dictionary_star":
        this.dictionary = this.dictionary.map((e) => (e.id === args.id ? { ...e, starred: args.starred as boolean } : e));
        return null;
      case "dictionary_delete":
        this.dictionary = this.dictionary.filter((e) => e.id !== args.id);
        return null;
      case "dictionary_import":
        return this.importWords();
      case "dictionary_export":
        return this.dictionary.length;
      case "snippets_list":
        return [...this.snippets].sort((a, b) => a.phrase.localeCompare(b.phrase));
      case "snippet_add":
        return this.saveSnippet(null, args.phrase as string, args.text as string);
      case "snippet_update":
        return this.saveSnippet(args.id as number, args.phrase as string, args.text as string);
      case "snippet_delete":
        this.snippets = this.snippets.filter((s) => s.id !== args.id);
        return null;
      case "style_overview":
        return this.styleOverview();
      case "style_set_tone":
        this.tones.set(args.category as Category, args.tone as StyleTone);
        return this.styleOverview();
      case "style_set_app":
        this.setApp(args.app as string, args.category as Category | null);
        return this.styleOverview();
      case "plugin:opener|open_url":
        window.open(args.url as string, "_blank", "noopener");
        return null;
      case "plugin:dialog|open":
        return "C:\\Users\\asha\\Documents\\team-words.csv";
      case "plugin:dialog|save":
        return `C:\\Users\\asha\\Documents\\${(args.options as { defaultPath?: string })?.defaultPath ?? "export"}`;
      case "plugin:window|is_maximized":
        return false;
      default:
        return null;
    }
  }

  setUpdate(status: UpdateStatus) {
    this.update = status;
    void events.updateChanged.emit(status);
  }

  async checkForUpdate(): Promise<UpdateStatus> {
    this.setUpdate({ state: "checking" });
    await wait(900);
    this.setUpdate(this.newer ?? { state: "upToDate" });
    return this.update;
  }

  /** Downloads, then stays "installing", since there's no app to restart. */
  installUpdate() {
    const found = this.update;
    if (found.state !== "available") return;
    let percent = 0;
    const timer = window.setInterval(() => {
      percent = Math.min(100, percent + 8);
      this.setUpdate({ state: "downloading", version: found.version, percent });
      if (percent === 100) {
        window.clearInterval(timer);
        this.setUpdate({ state: "installing", version: found.version });
      }
    }, 250);
  }

  overview(): ModelsOverview {
    const models = CATALOG.map((m) => {
      const installed = this.installed.has(m.id);
      return {
        ...m,
        installed,
        fit:
          m.id === "whisper-small"
            ? "recommended"
            : m.id === "whisper-large-v3-q5_0"
              ? "slow"
              : m.languages.length > 0
                ? "wrongLanguages"
                : "good",
      } satisfies ModelCard;
    });
    const current =
      this.settings.models.active ?? (this.installed.has("whisper-base") ? "whisper-base" : [...this.installed][0] ?? null);
    return {
      models,
      recommended: "whisper-small",
      current,
      downloads: [...this.downloads.values()].map((d) => d.progress),
      hardware: {
        cpu: "12th Gen Intel Core i7-1265U",
        cores: 10,
        threads: 12,
        ramMb: 16_384,
        availableRamMb: 7_310,
        avx2: true,
        gpus: [{ name: "Intel Iris Xe Graphics", vramMb: 128, dedicated: false }],
      },
      engine: {
        running: this.installed.size > 0,
        variant: "cpu",
        gpuBuild: false,
        gpuName: null,
        loaded: current ? [current] : [],
        vad: true,
      },
    };
  }

  /** A rough copy of `router::preview`. */
  routes(settings: Settings): LanguageRoutes {
    const { speak, current, output, alternate } = settings.languages;
    const consented = settings.privacy.consented.includes("sarvam");
    const toSarvam = (code: string) => consented && speak.some((l) => l.code === code && l.engine === "sarvam");
    const listen = current ? [current] : speak.map((l) => l.code);
    const sarvam = listen.filter(toSarvam);
    const known = listen.filter((c) => !toSarvam(c) && LANGUAGES.find((l) => l.code === c)?.whisper !== false);
    const local = known.length > 0 || sarvam.length > 0 ? known : listen.filter((c) => !toSarvam(c));
    const sources = listen.filter((c) => sarvam.includes(c) || local.includes(c));
    const cloud = sarvam.length > 0 && local.length === 0 ? "Sarvam" : null;
    const preview = (target: string | null): RoutePreview => {
      const to = target && !(sources.length === 1 && sources[0] === target) ? target : null;
      const from = sources.length === 1 ? sources[0].toUpperCase() : "Auto";
      const label = to ? `${from} → ${to.toUpperCase()}` : from;
      const english = sources.every((c) => c === "en");
      const model =
        english && this.installed.has("parakeet-tdt-0.6b-v3")
          ? "parakeet-tdt-0.6b-v3"
          : (["whisper-small", "whisper-base"].find((id) => this.installed.has(id)) ?? null);
      const byok = this.sttOverview().ready;
      if (!model && !cloud && !byok) {
        const problem = this.installed.size > 0 ? "None of your installed models understands Hindi." : "No speech model is installed yet.";
        return { label, model: null, translation: "none", sarvam: [], cloud: null, service: null, translator: null, problem };
      }
      // A service with the user's key hears what Sarvam doesn't.
      const serviceCloud = byok?.cloud && sarvam.length === 0 ? byok.name : null;
      const speechToEnglish = byok?.translates ?? true;
      const translation = !to ? "none" : to === "en" && speechToEnglish ? "speech" : "text";
      const sarvamText = consented && this.keys.has("sarvam") ? "Sarvam" : null;
      const ai = settings.polish.translate ? (this.aiOverview().ready?.name ?? null) : null;
      const aiFirst = settings.polish.level === "smart";
      const translator = translation !== "text" ? null : aiFirst ? (ai ?? sarvamText) : (sarvamText ?? ai);
      return {
        label,
        model,
        translation,
        sarvam,
        cloud: cloud ?? serviceCloud,
        service: byok?.name ?? null,
        translator,
        problem: null,
      };
    };
    return { dictate: preview(output), alternate: alternate ? preview(alternate) : null };
  }

  /** Sarvam keys that don't start with `sk_`, and other short keys, are refused as a wrong key would be. */
  testKey(provider: string, key: string | null): null {
    const spec = AI_PROVIDERS.find((p) => p.id === provider) ?? STT_PROVIDERS.find((p) => p.id === provider);
    const name = spec?.name ?? provider;
    if (key === null && spec && spec.key !== "required") return null;
    const known = key === null ? this.keys.has(provider) : provider === "sarvam" ? key.startsWith("sk_") : key.length >= 8;
    if (!known) {
      throw {
        message: key === null ? `Add your ${name} API key first.` : `${name} didn't accept your API key.`,
        keyProblem: true,
      };
    }
    return null;
  }

  /** A copy of `ipc::stt_overview` and `stt::chosen`. */
  sttOverview(): SttOverview {
    const { transcription, privacy } = this.settings;
    const spec = STT_PROVIDERS.find((p) => p.id === transcription.provider);
    let ready: SttOverview["ready"] = null;
    let problem: string | null = null;
    if (spec) {
      const base = spec.endpoint ? (transcription.endpoint ?? "") : "cloud";
      const cloud = !spec.endpoint || !isLocalUrl(base);
      const model = transcription.model ?? spec.models[0];
      if (!base) problem = "Your server's address is missing.";
      else if (cloud && !privacy.consented.includes(`stt:${spec.id}`))
        problem = `Joona doesn't have your permission to send recordings to ${spec.name}.`;
      else if (!model) problem = "No model is picked.";
      else if (spec.key === "required" && !this.keys.has(spec.id)) problem = `Your ${spec.name} API key is missing.`;
      else
        ready = { provider: spec.id, name: spec.name, model, cloud, translates: spec.translates.includes(model) };
    }
    return { providers: STT_PROVIDERS, ready, problem };
  }

  /** A copy of `ipc::ai_overview` and `ai::chosen`. */
  aiOverview(): AiOverview {
    const { polish, privacy } = this.settings;
    const spec = AI_PROVIDERS.find((p) => p.id === polish.provider);
    let ready: AiOverview["ready"] = null;
    let problem: string | null = null;
    if (spec) {
      const base = (spec.endpoint && polish.endpoint) || spec.defaultEndpoint || "";
      const cloud = spec.endpoint ? !isLocalUrl(base) : !spec.local;
      const model = polish.model ?? spec.models[0];
      if (!base && !spec.local) problem = "Your server's address is missing.";
      else if (cloud && !privacy.consented.includes(`ai:${spec.id}`))
        problem = `Joona doesn't have your permission to send text to ${spec.name}.`;
      else if (spec.key === "required" && !this.keys.has(spec.id)) problem = `Your ${spec.name} API key is missing.`;
      else if (spec.id === LOCAL_AI && !this.localAi) problem = "The local AI model isn't downloaded.";
      else if (!model) problem = "No model is picked.";
      else ready = { provider: spec.id, name: spec.name, model, cloud };
    }
    return {
      providers: AI_PROVIDERS,
      ready,
      problem,
      local: {
        ...LOCAL_AI_INFO,
        installed: this.localAi,
        running: this.localAi && polish.provider === LOCAL_AI,
        download: this.downloads.get(LOCAL_AI_DOWNLOAD)?.progress ?? null,
      },
    };
  }

  async aiModels(provider: string, endpoint: string | null): Promise<string[]> {
    const spec = AI_PROVIDERS.find((p) => p.id === provider);
    await wait(700);
    if (!spec) return [];
    if (spec.endpoint && !spec.defaultEndpoint && !endpoint?.trim()) {
      throw { message: "Type your server's address first.", keyProblem: false };
    }
    if (spec.id === "ollama") return ["llama3.2:3b", "qwen2.5:7b", "gemma3:4b"];
    if (spec.endpoint) return ["qwen2.5-7b-instruct", "llama-3.1-8b-instruct"];
    return [...spec.models, `${spec.models[0]}-latest`];
  }

  async aiTest(): Promise<AiSample> {
    const { ready, problem } = this.aiOverview();
    if (!ready) throw { message: problem ?? "No AI model is set up.", keyProblem: false };
    const ms = ready.cloud ? 840 : 1900;
    await wait(ms);
    return { ...AI_SAMPLE, ms };
  }

  sortedDictionary(): DictionaryEntry[] {
    const word = (e: DictionaryEntry) => e.word.toLowerCase();
    return [...this.dictionary].sort(
      (a, b) => Number(b.starred) - Number(a.starred) || b.uses - a.uses || word(a).localeCompare(word(b)),
    );
  }

  /** A few of the checks in `store/dictionary.rs`. */
  saveWord(id: number | null, word: string, heard: string | null): DictionaryEntry {
    const clean = word.trim();
    const wrong = heard?.trim() || null;
    if (!clean) throw "Type a word or phrase.";
    if (wrong?.toLowerCase() === clean.toLowerCase()) throw `That's already how “${clean}” is written.`;
    const same = (e: DictionaryEntry) =>
      e.id !== id && e.word.toLowerCase() === clean.toLowerCase() && (e.heard ?? "") === (wrong ?? "");
    if (this.dictionary.some(same)) throw `“${clean}” is already in your dictionary.`;
    const old = this.dictionary.find((e) => e.id === id);
    const entry: DictionaryEntry = {
      id: old?.id ?? Math.max(0, ...this.dictionary.map((e) => e.id)) + 1,
      word: clean,
      heard: wrong,
      starred: old?.starred ?? false,
      uses: old?.uses ?? 0,
      lastUsed: old?.lastUsed ?? null,
      createdAt: old?.createdAt ?? Date.now(),
    };
    this.dictionary = [...this.dictionary.filter((e) => e.id !== entry.id), entry];
    return entry;
  }

  importWords(): { added: number; skipped: number } {
    const words = ["Lakshmi", "Chennai", "Figma", "Supabase", "Tauri"];
    let added = 0;
    for (const word of words) {
      try {
        this.saveWord(null, word, null);
        added += 1;
      } catch {
        // Already there.
      }
    }
    return { added, skipped: words.length - added + 1 };
  }

  saveSnippet(id: number | null, phrase: string, text: string): Snippet {
    const cue = phrase.trim();
    if (!cue) throw "Type the phrase you'll say.";
    if (!text.trim()) throw "Type the text Joona should type.";
    if ([...text].length > 4_000) throw "A snippet can be up to 4000 characters long.";
    if (this.snippets.some((s) => s.id !== id && s.phrase.toLowerCase() === cue.toLowerCase())) {
      throw `You already have a snippet for “${cue}”.`;
    }
    const old = this.snippets.find((s) => s.id === id);
    const snippet: Snippet = {
      id: old?.id ?? Math.max(0, ...this.snippets.map((s) => s.id)) + 1,
      phrase: cue,
      text: text.trimEnd(),
      uses: old?.uses ?? 0,
      lastUsed: old?.lastUsed ?? null,
      createdAt: old?.createdAt ?? Date.now(),
    };
    this.snippets = [...this.snippets.filter((s) => s.id !== snippet.id), snippet];
    return snippet;
  }

  /** A rough copy of `StyleMap::rules` and `StyleMap::overview`. */
  styleOverview(): StyleOverview {
    const rules: AppRule[] = [
      ...BUILT_IN_APPS.map(([app, name, category]) => ({ app, name, website: false, category })),
      ...BUILT_IN_SITES.map(([site, category]) => ({ app: site, name: site, website: true, category })),
    ].map((rule) => ({
      ...rule,
      category: this.chosen.get(rule.app) ?? rule.category,
      builtIn: true,
      changed: this.chosen.has(rule.app),
    }));
    for (const [app, category] of this.chosen) {
      if (rules.some((r) => r.app === app)) continue;
      const website = !app.endsWith(".exe");
      const name = website ? app : (RECENT_APPS.find((r) => r.app === app)?.name ?? app.replace(/\.exe$/, ""));
      rules.push({ app, name, website, category, builtIn: false, changed: true });
    }
    rules.sort((a, b) => a.name.localeCompare(b.name) || Number(a.website) - Number(b.website));
    return {
      categories: (Object.keys(EXAMPLES) as Category[]).map((category) => ({
        category,
        tone: this.tones.get(category) ?? (category === "personal" || category === "work" ? "casual" : "formal"),
        previews: TONES.map((tone) => ({ tone, text: styled(EXAMPLES[category], tone) })),
      })),
      apps: rules,
      recent: RECENT_APPS.filter((r) => !rules.some((rule) => rule.app === r.app)),
    };
  }

  /** Like `Db::set_app_category`: websites keep their case, programs are lowercase. */
  setApp(app: string, category: Category | null) {
    const typed = app.trim();
    if (!typed) throw "Type the name of an app or website.";
    const program = typed.toLowerCase().endsWith(".exe");
    const known = [...BUILT_IN_SITES.map(([site]) => site), ...this.chosen.keys()].find(
      (k) => k.toLowerCase() === typed.toLowerCase(),
    );
    const key = program ? typed.toLowerCase() : (known ?? typed);
    for (const k of [...this.chosen.keys()]) if (k.toLowerCase() === key.toLowerCase()) this.chosen.delete(k);
    if (category) this.chosen.set(key, category);
  }

  download(id: string) {
    const size = id === LOCAL_AI_DOWNLOAD ? LOCAL_AI_INFO.size : CATALOG.find((m) => m.id === id)?.downloadSize;
    if (!size) return;
    const previous = this.downloads.get(id);
    window.clearInterval(previous?.timer);
    const progress: Progress = {
      id,
      stage: "downloading",
      done: previous?.progress.stage === "cancelled" ? previous.progress.done : 0,
      total: size,
      bytesPerSec: 0,
      error: null,
    };
    const step = Math.max(1, Math.round(size / 45));
    const timer = window.setInterval(() => {
      if (progress.stage === "downloading") {
        progress.done = Math.min(progress.total, progress.done + step);
        progress.bytesPerSec = 11_800_000 + Math.round(Math.random() * 1_600_000);
        if (progress.done >= progress.total) progress.stage = "verifying";
      } else {
        window.clearInterval(timer);
        progress.stage = "done";
        this.downloads.delete(id);
        if (id === LOCAL_AI_DOWNLOAD) {
          this.localAi = true;
          void events.aiChanged.emit();
        } else {
          this.installed.add(id);
        }
        void events.modelsChanged.emit();
      }
      void events.modelDownload.emit({ ...progress });
    }, 150);
    this.downloads.set(id, { progress, timer });
    void events.modelDownload.emit({ ...progress });
  }

  cancel(id: string) {
    const running = this.downloads.get(id);
    if (!running) return;
    window.clearInterval(running.timer);
    running.progress.stage = "cancelled";
    running.progress.bytesPerSec = 0;
    void events.modelDownload.emit({ ...running.progress });
  }

  add(text: string): Entry {
    const words = text.split(/\s+/).length;
    const entry: Entry = {
      id: Math.max(0, ...this.entries.map((e) => e.id)) + 1,
      createdAt: Date.now(),
      app: "joona.exe",
      appName: "Joona",
      raw: text,
      text,
      language: "en",
      output: null,
      words,
      audioMs: Math.round((words / 2.3) * 1000),
      model: CATALOG.find((m) => this.installed.has(m.id))?.name ?? "Whisper Base",
      cloud: null,
      mode: "dictate",
      delivery: "pasted",
      fixes: 0,
      category: null,
      audio: false,
    };
    this.entries.unshift(entry);
    return entry;
  }

  list(search: string, before: number | null, limit: number): Entry[] {
    const query = search.trim().toLowerCase();
    return this.entries
      .filter((e) => before === null || e.createdAt < before)
      .filter((e) => !query || [e.text, e.raw, e.appName ?? ""].some((t) => t.toLowerCase().includes(query)))
      .slice(0, limit);
  }

  homeStats(): HomeStats {
    const monday = new Date();
    monday.setHours(0, 0, 0, 0);
    monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
    const week = this.activity.filter((d) => new Date(`${d.date}T00:00`) >= monday);
    return {
      wordsThisWeek: week.reduce((sum, d) => sum + d.words, 0),
      wpm: this.activity.length ? 138 : 0,
      streak: streak(this.activity),
      totalDictations: this.activity.reduce((sum, d) => sum + d.dictations, 0),
    };
  }

  insights(): Insights {
    const totalWords = this.activity.reduce((sum, d) => sum + d.words, 0);
    const totalDictations = this.activity.reduce((sum, d) => sum + d.dictations, 0);
    const share = [0.34, 0.22, 0.17, 0.12, 0.09, 0.06];
    const apps = [
      { app: "slack.exe", name: "Slack", category: "work" },
      { app: "chrome.exe", name: "Google Chrome", category: "other" },
      { app: "Code.exe", name: "Visual Studio Code", category: "other" },
      { app: "OUTLOOK.EXE", name: "Outlook", category: "email" },
      { app: "WINWORD.EXE", name: "Word", category: "other" },
      { app: "WhatsApp.exe", name: "WhatsApp", category: "personal" },
    ];
    return {
      totalWords,
      totalDictations,
      wpm: totalDictations ? 138 : 0,
      minutesSaved: Math.round(totalWords / 40 - totalWords / 138),
      fixes: Math.round(totalDictations * 0.8),
      apps: totalDictations
        ? apps.map((a, i) => ({
            ...a,
            words: Math.round(totalWords * share[i]),
            dictations: Math.round(totalDictations * share[i]),
          }))
        : [],
      days: this.activity,
      currentStreak: streak(this.activity),
      longestStreak: Math.max(streak(this.activity), totalDictations ? 23 : 0),
      cloudPercent: totalDictations ? 8 : 0,
    };
  }

  record() {
    this.stopRecording();
    const steps: [number, Shortcut | null][] = [
      [600, { ctrl: false, win: false, alt: false, shift: false, key: 0x77 }],
      [1300, null],
    ];
    let last: Shortcut | null = null;
    for (const [delay, shortcut] of steps) {
      this.recordTimers.push(
        window.setTimeout(() => {
          if (shortcut) {
            last = shortcut;
            void events.shortcutRecording.emit({ kind: "preview", shortcut, label: shortcutLabel(shortcut) });
          } else if (last) {
            void events.shortcutRecording.emit({ kind: "done", shortcut: last, label: shortcutLabel(last), trigger: true });
          }
        }, delay),
      );
    }
  }

  stopRecording() {
    const recording = this.recordTimers.length > 0;
    this.recordTimers.forEach((t) => window.clearTimeout(t));
    this.recordTimers = [];
    if (recording) void events.shortcutRecording.emit({ kind: "cancelled" });
  }

  startMicTest() {
    window.clearInterval(this.micTimer);
    let syllableEnds = 0;
    let loud = 0;
    this.micTimer = window.setInterval(() => {
      const now = performance.now();
      if (now > syllableEnds) {
        loud = Math.random() < 0.2 ? 0.03 : 0.3 + Math.random() * 0.55;
        syllableEnds = now + 150 + Math.random() * 220;
      }
      void events.micTestLevel.emit(loud * (0.85 + Math.random() * 0.3));
    }, 33);
  }
}

/** The same as `Settings::default()` in Rust. */
export function defaultSettings(): Settings {
  return {
    version: 1,
    onboarded: false,
    general: {
      launchAtLogin: true,
      bar: "always",
      sounds: true,
      theme: "system",
      microphone: null,
      paused: false,
      checkUpdates: true,
    },
    shortcuts: {
      trigger: { ctrl: true, win: true, alt: false, shift: false, key: null },
      spaceForHandsFree: true,
      doubleTapForHandsFree: true,
      pasteLast: { ctrl: false, win: false, alt: true, shift: true, key: 0x5a },
    },
    languages: { speak: [{ code: "en", engine: "local" }], output: null, script: "native", alternate: null, current: null },
    models: { active: null, english: null, useGpu: false, idleUnloadMinutes: 10, preload: true, beamSize: null, threads: null },
    insertion: {
      method: "paste",
      restoreClipboard: true,
      trailingSpace: true,
      pasteTimeoutMs: 1500,
      restoreDelayMs: 150,
      overrides: [],
    },
    polish: { level: "basic", provider: null, model: null, endpoint: null, translate: true },
    transcription: { provider: null, model: null, endpoint: null },
    privacy: {
      keepHistory: true,
      historyDays: 0,
      saveAudio: false,
      fallbackToLocal: true,
      fallbackToast: true,
      consented: [],
    },
    experimental: { commandMode: false, pressEnter: false },
  };
}

const APP_INFO: AppInfo = {
  version: "0.1.0",
  os: "Windows 11 (build 26200)",
  mica: false,
  firstName: "Asha",
  settingsFile: "C:\\Users\\asha\\AppData\\Roaming\\Joona\\settings.json",
  dataDir: "C:\\Users\\asha\\AppData\\Local\\Joona",
  modelsDir: "C:\\Users\\asha\\AppData\\Local\\Joona\\models",
  logsDir: "C:\\Users\\asha\\AppData\\Local\\Joona\\logs",
};

/** Part of `APPS` and `SITES` in `style.rs`. */
const BUILT_IN_APPS: [string, string, Category][] = [
  ["whatsapp.exe", "WhatsApp", "personal"],
  ["telegram.exe", "Telegram", "personal"],
  ["discord.exe", "Discord", "personal"],
  ["slack.exe", "Slack", "work"],
  ["ms-teams.exe", "Microsoft Teams", "work"],
  ["zoom.exe", "Zoom", "work"],
  ["olk.exe", "Outlook", "email"],
  ["thunderbird.exe", "Thunderbird", "email"],
];
const BUILT_IN_SITES: [string, Category][] = [
  ["WhatsApp", "personal"],
  ["Instagram", "personal"],
  ["Slack", "work"],
  ["LinkedIn", "work"],
  ["Gmail", "email"],
  ["Outlook", "email"],
];
const RECENT_APPS: RecentApp[] = [
  { app: "code.exe", name: "VS Code" },
  { app: "winword.exe", name: "Word" },
  { app: "notion.exe", name: "Notion" },
  { app: "windowsterminal.exe", name: "Terminal" },
];

/** The same as `Category::example` in Rust. */
const EXAMPLES: Record<Category, string> = {
  personal: "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you.",
  work: "Hi team, the deck is ready for review. Let me know what you think by Friday.",
  email: "Hi Priya, thanks for sending the report. I'll go through it today and get back to you.",
  other: "Remember to book the train tickets for Saturday morning.",
};
const TONES: StyleTone[] = ["formal", "casual", "veryCasual", "excited"];

/** Enough of `style::apply` for the examples. */
function styled(text: string, tone: StyleTone): string {
  const loose = text.replace(/^(hey|hi|hello),/i, "$1").replace(/\.$/, "");
  switch (tone) {
    case "formal":
      return text;
    case "casual":
      return loose;
    case "veryCasual":
      return loose.replace(/(^|[.?!]\s+)([A-Z])(?=[a-z])/g, (_, before: string, letter: string) => before + letter.toLowerCase());
    case "excited":
      return text.replace(/\.$/, "!");
  }
}

function sampleDictionary(now: number): DictionaryEntry[] {
  const words: [string, string | null, boolean, number][] = [
    ["Joona", "Juna", true, 31],
    ["Priya Sharma", null, true, 12],
    ["GitHub", null, false, 22],
    ["Kubernetes", "cuber netes", false, 9],
    ["Bengaluru", null, false, 4],
    ["Sarvam", null, false, 3],
    ["Thiruvananthapuram", null, false, 1],
    ["PostgreSQL", "postgres sequel", false, 0],
  ];
  return words.map(([word, heard, starred, uses], i) => ({
    id: i + 1,
    word,
    heard,
    starred,
    uses,
    lastUsed: uses ? now - (i + 1) * 3_600_000 : null,
    createdAt: now - (30 - i) * DAY_MS,
  }));
}

function sampleSnippets(now: number): Snippet[] {
  const snippets: [string, string, number][] = [
    ["my address", "Flat 4B, Lakeview Apartments\n12 MG Road\nBengaluru 560001", 14],
    ["my calendar", "You can book a time with me here: https://cal.com/asha", 6],
    ["sign off", "Thanks,\nAsha", 21],
  ];
  return snippets.map(([phrase, text, uses], i) => ({
    id: i + 1,
    phrase,
    text,
    uses,
    lastUsed: now - (i + 2) * 3_600_000,
    createdAt: now - (20 - i) * DAY_MS,
  }));
}

type Sample = Partial<Entry> & Pick<Entry, "text" | "appName"> & { minutesAgo: number };

const SAMPLES: Sample[] = [
  {
    minutesAgo: 4,
    app: "slack.exe",
    appName: "Slack",
    raw: "um running like ten minutes late uh start without me and I'll catch up on the notes",
    text: "Running ten minutes late. Start without me and I'll catch up on the notes.",
    model: "Parakeet v3",
    language: "en",
    fixes: 3,
    category: "work",
  },
  {
    minutesAgo: 31,
    app: "chrome.exe",
    appName: "Google Chrome",
    raw: "कल की मीटिंग को शुक्रवार तक आगे बढ़ा दीजिए और सबको बता दीजिए",
    text: "Please move tomorrow's meeting to Friday and let everyone know.",
    language: "hi",
    output: "en",
    model: "Whisper Base",
  },
  {
    minutesAgo: 75,
    app: "Code.exe",
    appName: "Visual Studio Code",
    raw: "add a retry with exponential backoff around the fetch call and log the final error",
    text: "Add a retry with exponential backoff around the fetch call and log the final error.",
    model: "Parakeet v3",
    language: "en",
    fixes: 1,
    category: "other",
  },
  {
    minutesAgo: 60 * 20,
    app: "WhatsApp.exe",
    appName: "WhatsApp",
    raw: "நாளைக்கு காலை பத்து மணிக்கு வருகிறேன்",
    text: "I'll come tomorrow at ten in the morning.",
    language: "ta",
    output: "en",
    model: "Sarvam Saaras",
    cloud: "Sarvam",
    category: "personal",
  },
  {
    minutesAgo: 60 * 23,
    app: "OUTLOOK.EXE",
    appName: "Outlook",
    raw: "hi priya thanks for sending the draft over I went through it and the numbers in section three look off can we go over them on thursday no wait friday afternoon",
    text: "Hi Priya, thanks for sending the draft over. I went through it and the numbers in section three look off. Can we go over them on Friday afternoon?",
    model: "Whisper Base",
    language: "en",
    fixes: 4,
    category: "email",
  },
  {
    minutesAgo: 60 * 24 * 3 + 40,
    app: "OUTLOOK.EXE",
    appName: "Outlook",
    raw: "make this friendlier",
    text: "Thanks so much for the quick turnaround, really appreciate it! Let me know if anything else comes up.",
    mode: "command",
    model: "Whisper Base",
    language: "en",
    category: "email",
  },
  {
    minutesAgo: 60 * 24 * 3 + 95,
    app: "WindowsTerminal.exe",
    appName: "Windows Terminal",
    raw: "git rebase interactive head tilde three",
    text: "git rebase -i HEAD~3",
    delivery: "copied",
    model: "Parakeet v3",
    language: "en",
    fixes: 1,
  },
  {
    minutesAgo: 60 * 24 * 9 + 200,
    app: "WINWORD.EXE",
    appName: "Word",
    raw: "the pilot ran for six weeks across three districts attendance went up by eighteen percent and teachers said the voice notes saved them about an hour a day",
    text: "The pilot ran for six weeks across three districts. Attendance went up by 18%, and teachers said the voice notes saved them about an hour a day.",
    model: "Whisper Base",
    language: "en",
    fixes: 2,
    category: "other",
  },
];

function sampleEntries(now: number): Entry[] {
  return SAMPLES.map((s, i) => {
    const words = s.text.split(/\s+/).length;
    return {
      id: SAMPLES.length - i,
      createdAt: now - s.minutesAgo * 60_000,
      app: s.app ?? null,
      appName: s.appName,
      raw: s.raw ?? s.text,
      text: s.text,
      language: s.language ?? null,
      output: s.output ?? null,
      words,
      audioMs: Math.round((words / 2.3) * 1000),
      model: s.model ?? "Whisper Base",
      cloud: s.cloud ?? null,
      mode: s.mode ?? "dictate",
      delivery: s.delivery ?? "pasted",
      fixes: s.fixes ?? 0,
      category: s.category ?? null,
      audio: false,
    };
  });
}

/** A year of plausible use: most weekdays, fewer weekends, every day lately. */
function sampleActivity(now: number): DayActivity[] {
  let seed = 7;
  const random = () => {
    seed = (seed * 16807) % 2147483647;
    return seed / 2147483647;
  };
  const days: DayActivity[] = [];
  for (let ago = 300; ago >= 0; ago--) {
    const date = new Date(now - ago * DAY_MS);
    const weekend = date.getDay() === 0 || date.getDay() === 6;
    const active = ago < 12 || random() < (weekend ? 0.25 : 0.8);
    if (!active) continue;
    const dictations = 2 + Math.round(random() * (weekend ? 6 : 28));
    days.push({ date: localDate(date), dictations, words: dictations * (14 + Math.round(random() * 22)) });
  }
  return days;
}

function localDate(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

function streak(days: DayActivity[]): number {
  const active = new Set(days.map((d) => d.date));
  let count = 0;
  const day = new Date();
  if (!active.has(localDate(day))) day.setDate(day.getDate() - 1);
  while (active.has(localDate(day))) {
    count += 1;
    day.setDate(day.getDate() - 1);
  }
  return count;
}
