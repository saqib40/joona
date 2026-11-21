# Plan: selection-aware AI commands with Sarvam chat models

Status: proposed
Scope: Windows first, macOS parity where the existing code already supports it
Last researched: 21 November 2026

## 1. Summary

Joona already has the selection-aware command feature the request describes:
hold **Ctrl+Win+Alt**, speak an instruction, and the selected text is replaced
by the AI model's answer. It also already has a Sarvam chat provider entry, but
that entry still points at **`sarvam-m`**, which Sarvam has deprecated and
removed from the API. So the work is not "build Command Mode from scratch". It
is:

1. replace the dead Sarvam model ID with Sarvam's current chat models,
2. make Sarvam requests behave correctly for editing (reasoning off, token
   limits, language gating),
3. make the Sarvam choice visible and understandable in the UI,
4. optionally add fixed shortcut chords such as **Ctrl+Win+Alt+T** so common
   actions do not need a spoken instruction.

The recommended architecture is to reuse the existing `providers::llm` path.
Sarvam's chat API is OpenAI-compatible, and `Llm` already sends the
`api-subscription-key` header for Sarvam. A separate Sarvam chat client would
duplicate code and split error handling.

## 2. What the code does today

### 2.1 Command Mode

- Default trigger is `Ctrl+Win` (`hotkeys/gesture.rs`, `Shortcut::CTRL_WIN`).
- Holding the trigger starts dictation. Holding it plus `Alt` switches to
  `Mode::Command` (`GestureConfig::command_mode`, `mode_from_held`).
- On release, the spoken instruction is transcribed. `Session::read_selection`
  then reads the foreground app's selection with a simulated `Ctrl+C`
  (`insert/windows/mod.rs`, `Inserter::copy_selection`), restores the clipboard,
  and calls `Session::run_command`.
- `ai::Command` builds a prompt that treats the selection as material and the
  instruction as the task. The answer is pasted over the selection, or typed at
  the cursor when nothing is selected (`ai/command.rs`).
- Command Mode already requires an AI model before recording starts
  (`Session::start` calls `self.ai(...)`), and shows a warning toast when none is
  configured.
- The AI consent is separate from Sarvam's speech consent:
  `privacy.consented` holds `ai:sarvam` for chat and `sarvam` for speech.

### 2.2 Sarvam integration today

- `providers/sarvam.rs` covers speech-to-text (`saaras:v4`), translation
  (`mayura:v1`, `sarvam-translate:v1`) and romanization.
- `providers/llm.rs` already lists Sarvam as an AI provider:
  - base `https://api.sarvam.ai/v1`
  - `Api::OpenAi`
  - auth header `api-subscription-key` plus a Bearer header
  - models: `["sarvam-m"]`
- `ai::chosen` reads `settings.polish.provider`, resolves the `llm::Spec`, checks
  `ai:sarvam` consent, and checks that the key exists.
- `AiSheet` lists every `llm::Spec`, so Sarvam is selectable as an AI model
  today. `ai_test` sends a sample polish request through `ai::run`.
- `hub/demo.ts` mirrors the same stale `sarvam-m` entry for the demo UI.

### 2.3 Gaps that matter

| Gap | Effect |
| --- | --- |
| `sarvam-m` is deprecated and unavailable | Choosing Sarvam as the AI model fails on every request. |
| No `reasoning_effort` control | `sarvam-105b` reasons by default. A small `max_tokens` can be consumed by reasoning, returning empty `content` with `finish_reason: length`. |
| No Sarvam output-token cap | `Command::max_tokens` can reach 8192. Sarvam Starter keys allow 4096, so long selections can get a 400. |
| One model for polish and commands | The conversational variant and the flagship model have different strengths, but there is no way to pick per task. |
| No language gate for chat | Sarvam chat covers 10 Indian languages + English, while Saaras covers 22 + English. Unsupported languages silently go to a model that may not handle them. |
| Consent copy only mentions dictation | The AI consent text does not say that selected text is sent in Command Mode. |
| No fixed action chords | Every command needs a spoken instruction, even for common actions such as "fix grammar" or "translate". |
| `Ctrl+Win+Alt+letter` currently cancels | The gesture machine treats any non-modifier key within `ACCIDENTAL_WINDOW_MS` as an accidental shortcut. |

## 3. Research: Sarvam chat models, November 2026

### 3.1 Current models

Sarvam's own documentation now lists two chat models:

| Model ID | What it is | Context | Reasoning | Best for |
| --- | --- | --- | --- | --- |
| `sarvam-105b` | Flagship 105B MoE, trained from scratch, Apache 2.0 | 128K tokens | On by default (`reasoning_effort: low`); turn off with `reasoning_effort: null` | Complex reasoning, coding, long context, highest-quality edits |
| `sarvam-105b-conversations` | Post-trained variant of 105B for real-time dialogue and voice agents | 32K tokens | Always off; a `reasoning_effort` level is accepted but ignored | Natural, colloquial Indic responses; low-latency chat |

Both cover the 11-language set: Hindi, Bengali, Tamil, Telugu, Gujarati,
Kannada, Malayalam, Marathi, Punjabi, Odia and English. They accept native
script, romanized and code-mixed input.

Deprecated and no longer available through the API:

- `sarvam-m` (24B)
- `sarvam-30b`

Sarvam recommends migrating both to `sarvam-105b`.

### 3.2 API shape

- Endpoint: `POST https://api.sarvam.ai/v1/chat/completions`
- `sarvam-105b` is also served on `/v2/chat/completions`; the conversational
  variant is only on `/v1`.
- Auth: `api-subscription-key: <key>`; `Authorization: Bearer <key>` is also
  accepted for OpenAI-compatible tooling. Joona already sends both.
- Request is OpenAI-compatible: `model`, `messages`, `temperature`,
  `max_tokens`, `stream`, `n`, `stop`, `tools`, `response_format`.
- Response is OpenAI-compatible: `choices[0].message.content`,
  `choices[0].message.reasoning_content`, `finish_reason`, `usage`.
- `n` must be 1.
- Temperature range 0–2; `sarvam-105b` defaults to 1.0 with reasoning on and
  0.6 with reasoning off. `sarvam-105b-conversations` defaults to 0.6.
- `max_tokens` defaults to 2048. Tier caps: Starter 4096, Pro 16384, Business
  128000. Reasoning tokens count toward `max_tokens` and are billed as output.
- Errors: `403 invalid_api_key_error` for auth (not 401), `402
  insufficient_quota_error` for empty credits, `429` for account rate limits or
  model capacity, `500/502/503` for transient failures.
- Rate limit for `sarvam-105b` and `sarvam-105b-conversations`: 60 req/min on
  pay-as-you-go, 200 on Pro, 300 on Growth. Fine for interactive dictation.

### 3.3 Pricing

Introductory pricing through the end of 2026:

- Input: ₹10 per 1M tokens
- Cached input: ₹0.25 per 1M tokens
- Output: ₹25 per 1M tokens

A typical Command Mode request (roughly 1,000 input tokens and 300–500 output
tokens) costs about ₹0.02, or a fraction of a US cent. Even heavy use is cheap
compared with the speech-to-text cost.

### 3.4 Practical implications

1. **Use `sarvam-105b` with `reasoning_effort: null` for edits by default.**
   Reasoning is useful for hard questions, but it adds latency, costs output
   tokens, and can eat the whole `max_tokens` budget. A rewrite should not
   spend 800 tokens thinking.
2. **Offer `sarvam-105b-conversations` as the low-latency / conversational
   option.** It is the better fit for "reply to this message" or "make this
   sound friendlier", but it is tuned for dialogue, not necessarily for precise
   grammar edits. Benchmark both before making it the default.
3. **Cap Sarvam output at 4096 tokens in v1.** That works on every tier. If a
   request is cut off, the existing `finish_reason == "length"` path already
   reports it.
4. **Gate by language.** Sarvam chat covers 11 languages. Saaras covers 23. If
   the user dictates in Assamese, Urdu, Nepali, Konkani, Kashmiri, Sindhi,
   Sanskrit, Santali, Manipuri, Bodo, Maithili or Dogri, do not silently send
   the command to Sarvam chat. Fall back to the configured non-Sarvam model, or
   translate through Sarvam's translation endpoint first.
5. **Do not use chat for a plain translation preset.** Sarvam's dedicated
   translation models are cheaper and purpose-built. Use `Sarvam::translate`
   for the "Translate" quick action when the language pair is supported.

## 4. Product design

### 4.1 Spoken Command Mode (keep, improve)

Flow:

1. Select text in any app.
2. Hold **Ctrl+Win+Alt**.
3. Say what to do: "make this more professional", "reply to this", "make it
   shorter", "translate to Hindi".
4. Release. Joona reads the selection, sends the instruction and selection to
   the chosen model, and pastes the answer over the selection.
5. With nothing selected, the answer is typed at the cursor.

Improvements:

- When Sarvam is the chosen provider, the bar badge says **Sarvam** and the
  working state says something like "Sarvam is writing…".
- The consent sheet explicitly says that the selected text and the spoken
  instruction are sent, and that the recording is not.
- If the selection cannot be read (elevated app, terminal, password field), show
  a specific toast instead of silently running with no selection.
- If the instruction is empty or unintelligible, do not send an empty command.

### 4.2 Quick action chords (new, optional)

The same selection-reading and replacement pipeline, but the instruction is
fixed and no microphone is used.

Proposed defaults, all relative to the user's trigger:

| Shortcut | Action | Model path |
| --- | --- | --- |
| `Trigger+Alt+T` | Translate selection to the output language / English | Sarvam Translate when supported, otherwise chat |
| `Trigger+Alt+G` | Fix grammar and spelling | Chat |
| `Trigger+Alt+R` | Rewrite / improve wording | Chat |
| `Trigger+Alt+S` | Summarize | Chat |
| `Trigger+Alt+E` | Explain the selection | Chat |
| `Trigger+Alt+1…5` | User-defined actions | Chat |

Notes:

- The trigger is configurable, so on macOS this becomes `Ctrl+Cmd+Option+T`,
  etc. The existing `Shortcut::label` already maps `Win` to `Cmd` and `Alt` to
  `Option`.
- Quick actions require a selection. With nothing selected, show "Select some
  text first" rather than writing at the cursor, except for user-defined
  "write" actions.
- Quick actions should work even if no speech model is installed, because they
  never record audio.
- They should be configurable and easy to disable. `Ctrl+Win+Alt+letter` can
  collide with other tools (PowerToys, IDEs, keyboard layouts), so the UI must
  let the user change or turn them off.

### 4.3 Gesture-machine change required

Today, while the trigger is held, any non-modifier key within
`ACCIDENTAL_WINDOW_MS` (1 s) cancels the dictation and passes the key through.
That is deliberate: it keeps shortcuts such as `Ctrl+Win+D` working.

To support quick actions:

1. Add a map of `Shortcut` → `QuickAction` to `GestureConfig`.
2. In `GestureMachine::holding`, before the accidental-key branch, check whether
   the pressed key plus the currently held modifiers exactly matches a quick
   action.
3. If it matches, swallow the key, emit `Action::QuickAction { action }`, and
   move to `WaitRelease`.
4. If it does not match, keep the existing accidental-cancel behavior.
5. Keep `InjectMask` on Win release so the Start menu does not open.

This is a small, testable change in the pure state machine, and it does not
affect dictation.

## 5. Architecture

### 5.1 Reuse the LLM provider path

Do not add a second Sarvam client. The existing `Llm` already handles the
OpenAI-compatible request shape, the Sarvam auth header, timeouts, model lists
and error mapping.

Changes to `providers/llm.rs`:

- Update the Sarvam `Spec`:
  - summary: mention Sarvam-105B and Indian languages
  - `models: &["sarvam-105b", "sarvam-105b-conversations"]`
  - keep `base: "https://api.sarvam.ai/v1"`
  - add a reasoning policy (`Off` for Sarvam) and an output-token cap
    (`Some(4096)` for Sarvam)
- In `Llm::openai_chat`, add `reasoning_effort: null` when the spec's policy is
  `Off`. This is the documented way to turn reasoning off on `sarvam-105b`.
  `sarvam-105b-conversations` ignores it, so the same body is safe for both.
- Clamp `max_tokens` to the spec's cap before sending.
- Keep the existing `finish_reason == "length"` handling; add a clearer message
  for the "reasoning used the whole budget" case if `content` is empty.
- Update `Llm::models` only if Sarvam adds a model-list endpoint. For now the
  static list is correct and avoids an unnecessary request.
- Extend `error_from` tests for Sarvam's `model_rate_limit_exceeded_error` and
  `seat_budget_exhausted_error`. The current 429 → `RateLimited` mapping is
  acceptable; the message should be preserved.

### 5.2 Model choice and routing

Recommended v1: one model per provider, chosen by the existing `polish.model`
setting.

- Default Sarvam model: `sarvam-105b` with reasoning off.
- Advanced users can type `sarvam-105b-conversations` in the model field.

Recommended v2, if benchmarks justify it: a per-task default.

- Add `Task::{Polish, Command}` to `ai`.
- `ai::chosen(..., task)` picks:
  1. `settings.polish.model` if the user set one explicitly,
  2. the provider's command default for `Task::Command`,
  3. `spec.models[0]` otherwise.
- Add `Spec::command_model: Option<&'static str>`. For Sarvam this can be
  `sarvam-105b-conversations` once measured.
- Keep the UI simple: one model field, plus a small "Command Mode model"
  override for advanced users.

Do not make the conversational variant the default until it is benchmarked on
actual edit prompts. It is tuned for dialogue and may add conversational
phrasing that the prompt has to suppress.

### 5.3 Prompt changes

`ai/command.rs` already has the right structure: the selection is material, the
instruction is the task, the answer replaces the selection. Add Sarvam-aware
guidance to the system prompt:

- Keep the selection's script (Devanagari, Tamil, etc.) unless the instruction
  asks to transliterate.
- Accept romanized and code-mixed input; do not "correct" Hinglish into formal
  Hindi unless asked.
- Keep native numerals and currency formatting unless asked.
- Keep the selection's line breaks, lists and Markdown.
- Do not add an introduction, label or explanation.
- When the instruction is a question about the text, answer it instead of
  rewriting the text.

For the "Translate" quick action, bypass the chat prompt and call
`Sarvam::translate` when `can_translate(from, to)` is true. This uses the
dedicated translation model and avoids paying chat prices for a deterministic
translation.

### 5.4 Selection size and context limits

- Add a rough character guard based on the chosen model's context:
  - `sarvam-105b`: ~96,000 characters (128K tokens, leaving room for the
    answer), but practical UI selections are much smaller.
  - `sarvam-105b-conversations`: ~24,000 characters (32K tokens).
- If the selection is too large, show a toast and either:
  - truncate with a clear warning, or
  - suggest `sarvam-105b` / another provider.
- Do not chunk a rewrite silently: splitting a document and rewriting each
  piece loses cross-paragraph context.

### 5.5 Session flow for quick actions

Add a `Session` path that runs a command without a recording:

1. `Action::QuickAction` arrives from the gesture machine.
2. If a dictation is active, cancel/discard it without transcribing.
3. Create a command state with a synthetic instruction (for example "Fix
   grammar and spelling") and no `Heard` audio.
4. Reuse `read_selection` → `run_command` → `commanded` → `insert`.
5. Record the result in history as `Mode::Command`, with the fixed instruction
   as `raw` and the result as `text`. The selected text itself is not stored.

Refactor `run_command` so it can take an explicit instruction and selection
instead of requiring `d.heard` and `d.selection` to be populated by the speech
path. That keeps the spoken and quick paths on one execution path.

### 5.6 UI and consent

- `AiModels` / `AiSheet`:
  - update the Sarvam summary to Sarvam-105B
  - show both model IDs in the datalist
  - update the consent text: "the text of each dictation, and the text you
    select for Command Mode"
  - show a short note that Sarvam-105B is tuned for Indian languages and
    Hinglish
- `Providers` page:
  - the Sarvam card currently describes only speech and translation. When
    Sarvam is chosen as the AI model, mention "Command Mode and Smart polish"
    in the "used for" line.
- `hub/demo.ts`:
  - update the mock provider's summary and models so screenshots and tests match
    the real UI.
- `README.md`:
  - update the shortcut table and the Sarvam description.
- Settings:
  - add quick-action shortcuts and a cheat sheet under Experimental or
    Shortcuts.
  - keep Command Mode disabled when the trigger already uses Alt (existing
    behavior).

## 6. Implementation phases

### Phase 0 — Repair the existing Sarvam chat entry (1–2 days)

- Update `sarvam-m` → `sarvam-105b` / `sarvam-105b-conversations` in
  `providers/llm.rs` and `hub/demo.ts`.
- Add the reasoning policy and 4096-token cap.
- Add unit tests for the request body:
  - `POST /v1/chat/completions`
  - `api-subscription-key: test-key`
  - `model: sarvam-105b`
  - `reasoning_effort: null`
  - `max_tokens` clamped
- Add a mock response test for `sarvam-105b-conversations`.
- Update the AI consent copy.
- Manually verify with a real Sarvam key: `ai_test`, Smart polish, and Command
  Mode.

Exit criteria: selecting Sarvam in the AI model sheet works end to end for
Smart polish and Command Mode.

### Phase 1 — Harden Command Mode for Sarvam (2–3 days)

- Language gate and fallback for the 12 languages Sarvam chat does not cover.
- Selection size guard.
- Clear errors for 402 (no credits), 403 (bad key), 429 (rate/capacity), and
  timeouts.
- Better "empty answer" message when reasoning or `finish_reason: length`
  consumes the output.
- Benchmark `sarvam-105b` (reasoning off) against
  `sarvam-105b-conversations` on 20–30 real edit prompts in English, Hindi and
  Hinglish. Measure latency, acceptance, and output quality.
- Decide whether to add per-task model routing.

Exit criteria: Command Mode with Sarvam is reliable across English, Hindi and
Hinglish, and the failure modes are understandable.

### Phase 2 — Quick action chords (3–5 days)

- Add `QuickAction` to the gesture state machine and `GestureConfig`.
- Add the session path that runs a fixed command without recording.
- Add the settings UI and a cheat sheet.
- Add the translation preset using `Sarvam::translate`.
- Test on Windows with Notepad, VS Code, Chrome, Slack, Word and Outlook.

Exit criteria: `Ctrl+Win+Alt+G/T/S/R/E` work on selected text without speaking,
and do not break normal dictation or other `Ctrl+Win+…` shortcuts.

### Phase 3 — Optional polish

- Stream the answer into an overlay so the user sees progress before the paste.
- Multi-turn follow-up: "make it shorter" again on the last result.
- User-defined actions and per-app actions.
- A command palette (`Trigger+Alt+K`) instead of memorizing letter chords.
- Remote model catalog so Sarvam model deprecations do not require a release.

## 7. Testing plan

### Unit tests

- `GestureMachine`: quick action fires before the accidental-key branch; the key
  is swallowed; unmatched keys still cancel and pass through; `InjectMask` still
  fires.
- `llm::Spec`: Sarvam has a name, base, key URL, privacy URL, and a valid model
  list.
- `Llm::openai_chat`: Sarvam body includes `reasoning_effort: null`, uses the
  `api-subscription-key` header, and clamps `max_tokens`.
- `ai::chosen`: Sarvam needs `ai:sarvam` consent and a saved key; explicit
  `polish.model` wins.
- `ai::Command`: prompt keeps the selection as material; translation preset
  routes to the translation endpoint.

### Integration tests with a local mock server

Use the existing `providers/testing.rs` helpers (`serve_once`, `json_body`) to
assert:

- Sarvam request path, headers and JSON body.
- `sarvam-105b` response with `content` is accepted.
- Empty `content` with `finish_reason: "length"` is rejected with a useful
  message.
- `402` maps to `NoCredit`, `403 invalid_api_key_error` maps to `BadKey`, `429`
  maps to `RateLimited`.
- `sarvam-105b-conversations` works with the same body.

### Manual Windows matrix

- Apps: Notepad, VS Code, Chrome, Slack, Word, Outlook, Excel, Windows Terminal,
  an elevated app, a password field.
- Text: English, Hindi, Hinglish, Tamil, Bengali, Marathi, Odia, RTL text,
  Markdown, code blocks, long selections, whitespace-only selections.
- Clipboard: apps with clipboard managers, apps that read the clipboard late,
  apps where `Ctrl+C` does not work.
- Shortcuts: default `Ctrl+Win`, a custom trigger, trigger with Alt (Command Mode
  disabled), `Ctrl+Win+D` and other system shortcuts still work.
- Regression: dictation, hands-free, alternate output language, Smart polish,
  Sarvam translation, paste-last.

### Performance and cost

- Measure p50/p95 latency for `sarvam-105b` with reasoning off and
  `sarvam-105b-conversations`.
- Estimate cost per command from `usage.prompt_tokens` and
  `usage.completion_tokens`.
- Confirm that a 1,000-character selection stays under the 4096-token output cap
  for typical rewrites.

## 8. Risks and open questions

1. **Reasoning default.** `sarvam-105b` reasons by default. If
   `reasoning_effort: null` is not honored on `/v1`, the fallback is to use
   `sarvam-105b-conversations` or raise `max_tokens`. Verify with a real key
   before shipping.
2. **Language coverage.** Sarvam chat covers 11 languages, not the 23 Saaras
   covers. The fallback policy needs a product decision: switch to another
   provider, translate first, or show a warning.
3. **Shortcut collisions.** `Ctrl+Win+Alt+letter` can collide with other tools.
   Make quick actions configurable and easy to disable.
4. **Clipboard-based selection capture.** Simulated `Ctrl+C` is fragile in
   elevated apps, terminals and password fields. This is a pre-existing
   limitation; the UI should say so clearly.
5. **Model deprecations.** Sarvam-M was removed without a code change. Keep the
   model list easy to update, and consider a remote catalog or at least a
   startup warning when a configured model is no longer in `Spec::models`.
6. **Tier-dependent `max_tokens`.** Joona cannot detect the user's Sarvam tier.
   A 4096 cap is safe but may truncate very long rewrites. A retry-on-400 with a
   lower cap is an alternative.
7. **Conversational variant quality.** It is tuned for dialogue, not precise
   editing. Benchmark before making it the Command Mode default.
8. **Privacy expectations.** Users may assume "Sarvam for speech" also covers
   "Sarvam for text". Keep the separate `ai:sarvam` consent and explain the
   difference.

## 9. Recommended decisions

- **Reuse `providers::llm`.** Do not build a separate Sarvam chat client.
- **Ship `sarvam-105b` with `reasoning_effort: null` as the v1 default.**
  Expose `sarvam-105b-conversations` as a selectable model.
- **Cap Sarvam output at 4096 tokens in v1.** Revisit if users hit truncation.
- **Use `Sarvam::translate` for the Translate quick action.** Use chat for
  grammar, rewrite, summarize and explain.
- **Add quick action chords in Phase 2.** They are valuable, but the spoken
  Command Mode path and the Sarvam model repair come first.
- **Benchmark before per-task routing.** One model is simpler; add routing only
  if the conversational variant clearly wins for Command Mode.

## 10. Sources

- Sarvam models overview: <https://docs.sarvam.ai/api/getting-started/models>
- Sarvam-105B model page: <https://docs.sarvam.ai/api/token-factory/models/sarvam-105b>
- Chat Completions API: <https://docs.sarvam.ai/api/token-factory/chat-completions>
- Pricing and rate limits: <https://docs.sarvam.ai/api/getting-started/pricing>
- Errors and troubleshooting: <https://docs.sarvam.ai/api/getting-started/errors-troubleshooting>
- Authentication: <https://docs.sarvam.ai/api-reference/authentication>
- Deprecated Sarvam-M: <https://docs.sarvam.ai/api/getting-started/models/sarvam-m>
- Deprecated Sarvam-30B: <https://docs.sarvam.ai/api/getting-started/models/sarvam-30b>
