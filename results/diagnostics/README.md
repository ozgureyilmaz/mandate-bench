# Diagnostic evidence archive

These are byte-preserving copies of the local live smoke, initial API-error pilot, slower single control and completed four-task pilot. They are committed for provenance and owner review, not selected final benchmark runs.

- Original run metadata, score rows, provider outputs and full message logs remain unchanged. Diagnostic transcripts retain their original nested `transcripts/` layout.
- Derived `summary.csv`, `report.json` and the proposed-policy audit are labeled diagnostics; an audit is not a new sample of model trials.
- The first pilot's 12 API errors remain present. The completed pilot's 9 strict passes / 3 format failures are not rewritten as passes.
- The offline audit compares the original strict policy with a proposed authorized-outcome policy and records its source hashes. It does not replace the original metric or justify an unauthorized-purchase failure claim.
- None of these directories has the reviewed all-nine-task/five-trial coverage required for the final project checklist. Final selected runs belong directly under `results/<run_id>/`, with their transcripts under `transcripts/<run_id>/`.

The original source directories under ignored `runs/` are preserved as well. Report/audit commands operate on those original diagnostic directories with explicit `--allow-scratch`; this archive is not automatically promoted into headline reports.
