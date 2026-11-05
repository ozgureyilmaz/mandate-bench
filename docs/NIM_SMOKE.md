# Free comparison eligibility — Nov 4, 2026

The owner approved these three free NIM candidates, subject to an actual tool-calling smoke. The local preflight reported `NVIDIA_API_KEY missing`; it sent no request. The owner then explicitly chose **Leave NIM live checks pending**. No key was read into output and no live NIM model was tested. No comparison result or unsupported-model claim is made.

| Candidate ID | Live check | Comparison eligibility | Drop reason |
|---|---|---|---|
| moonshotai/kimi-k2.6 | Pending local key | Excluded until pass | No tool result; not classified unsupported |
| nvidia/nemotron-3-ultra-550b-a55b | Pending local key | Excluded until pass | No tool result; not classified unsupported |
| mistralai/mistral-large-4-1t-a52b | Pending local key | Excluded until pass | No tool result; not classified unsupported |

`configs/nim-smoke.toml` is a candidate-only smoke config: one request per model, serial, 6 RPM shared across the provider, existing retry/backoff with at most six retries, 2,048 output tokens. Smoke uses automatic tool choice, explicitly requests `smoke_echo({"value":"ready"})`, and accepts only exactly that untruncated tool call. A responding model that emits text, wrong arguments or truncates is excluded with `no_tool_call`; a terminal HTTP/transport error is excluded with `api_error`, retaining safe code/type diagnostics. Neither result is silently retried as another trial. Retryable attempts within the configured logical request remain visible. A successful future check writes a local eligibility record; `run` refuses a real NIM candidate without one. Smoke artifacts remain diagnostics and never count as benchmark trials.

After the owner resumes the pending checks, add `NVIDIA_API_KEY` locally in `.env`, then run:

```bash
cargo run --release --locked -- smoke --config configs/nim-smoke.toml --dry-run
cargo run --release --locked -- smoke --config configs/nim-smoke.toml
```

Model catalog documentation is descriptive, not proof of this account's hosted API access or tool support: [NVIDIA Nemotron model card](https://build.nvidia.com/nvidia/nemotron-3-ultra-550b-a55b/modelcard), [NVIDIA Mistral model card](https://build.nvidia.com/mistralai/mistral-large-4-1t-a52b/modelcard), [Moonshot's K2.6 model card](https://huggingface.co/moonshotai/Kimi-K2.6). Free NIM requests are outside the paid USD 1 ledger. No DeepSeek or other paid model is included in the active configs.
