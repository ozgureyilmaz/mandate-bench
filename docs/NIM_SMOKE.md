# Free comparison eligibility — Nov 5, 2026

The Nov 5 work order resumed NIM preflight for all three free candidates. The dry run reported `NVIDIA_API_KEY missing`, so live smoke was skipped. All candidates remain untested and require a passing tool call for admission.

| Candidate ID | Live check | Comparison eligibility | Drop reason |
|---|---|---|---|
| moonshotai/kimi-k2.6 | Skipped: missing local key | Excluded until pass | Untested |
| nvidia/nemotron-3-ultra-550b-a55b | Skipped: missing local key | Excluded until pass | Untested |
| mistralai/mistral-large-4-1t-a52b | Skipped: missing local key | Excluded until pass | Untested |

`configs/nim-smoke.toml` is a candidate-only smoke config: one request per model, serial, 6 RPM shared across the provider, existing retry/backoff with at most six retries, 2,048 output tokens. Smoke uses automatic tool choice, explicitly requests `smoke_echo({"value":"ready"})`, and accepts only exactly that untruncated tool call. A responding model that emits text, wrong arguments or truncates is excluded with `no_tool_call`; a terminal HTTP/transport error is excluded with `api_error`, retaining safe code/type diagnostics. Neither result is silently retried as another trial. Retryable attempts within the configured logical request remain visible. A successful future check writes a local eligibility record; `run` refuses a real NIM candidate without one. Smoke artifacts remain diagnostics and never count as benchmark trials.

After `NVIDIA_API_KEY` is set in local `.env`, run:

```bash
cargo run --release --locked -- smoke --config configs/nim-smoke.toml --dry-run
cargo run --release --locked -- smoke --config configs/nim-smoke.toml
```

Model catalog documentation is descriptive, not proof of this account's hosted API access or tool support: [NVIDIA Nemotron model card](https://build.nvidia.com/nvidia/nemotron-3-ultra-550b-a55b/modelcard), [NVIDIA Mistral model card](https://build.nvidia.com/mistralai/mistral-large-4-1t-a52b/modelcard), [Moonshot's K2.6 model card](https://huggingface.co/moonshotai/Kimi-K2.6). Free NIM requests are outside the paid OpenAI and Muse ledgers. No DeepSeek or other paid model is included in the active configs.
