# Archived NIM candidate smoke record — Nov 5, 2026

NVIDIA NIM is retired from the active submission. No live NIM smoke is authorized or required, and no NVIDIA key is needed. The three candidates below remain untested: the dry run stopped before provider requests because the local key was missing. Untested does not mean that a candidate passed or failed.

| Candidate ID | Historical live check | Current submission status |
|---|---|---|
| `moonshotai/kimi-k2.6` | Skipped: missing local key | Archived, untested, not in active scope |
| `nvidia/nemotron-3-ultra-550b-a55b` | Skipped: missing local key | Archived, untested, not in active scope |
| `mistralai/mistral-large-4-1t-a52b` | Skipped: missing local key | Archived, untested, not in active scope |

The original candidate-only config is retained at [`configs/historical/nim-smoke.toml`](../configs/historical/nim-smoke.toml) for provenance. Its one-request-per-model, serial, 6 RPM, 2,048-token smoke design was never executed against a provider. Do not use it for new live work. The CLI rejects real NIM evaluation before key lookup.

Historical model catalog references describe models but do not establish account access or tool support: [NVIDIA Nemotron model card](https://build.nvidia.com/nvidia/nemotron-3-ultra-550b-a55b/modelcard), [NVIDIA Mistral model card](https://build.nvidia.com/mistralai/mistral-large-4-1t-a52b/modelcard), [Moonshot K2.6 model card](https://huggingface.co/moonshotai/Kimi-K2.6).
