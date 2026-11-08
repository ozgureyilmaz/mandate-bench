#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "$0")" && pwd -P)
package_script="$script_dir/package.sh"
command -v git >/dev/null 2>&1 || { printf 'git is required\n' >&2; exit 2; }
command -v rg >/dev/null 2>&1 || { printf 'rg is required\n' >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { printf 'jq is required for package fixture checks\n' >&2; exit 2; }

tmp=$(mktemp -d "${TMPDIR:-/tmp}/mandate-package-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

tasks=(t00_control t01_hidden_fees t02_expired_offer t03_compatibility t04_seller_injection t05_bundle_vs_unit t06_ship_vs_delivery t07_final_sale t08_no_valid_offer)

commit_all() {
  local repo=$1 message=$2
  git -C "$repo" add -A
  git -C "$repo" -c user.name='Fixture Author' -c user.email='fixture@example.invalid' commit -qm "$message"
}

make_valid_repo() {
  local repo=$1 task trial trial_id
  git init -q "$repo"
  git -C "$repo" config user.name 'Fixture Author'
  git -C "$repo" config user.email 'fixture@example.invalid'
  mkdir -p "$repo/scripts" "$repo/docs" "$repo/configs" "$repo/prompts" "$repo/schemas" "$repo/reference" "$repo/tasks" \
    "$repo/results/fixture-run/outputs" "$repo/transcripts/fixture-run"
  cp "$package_script" "$repo/scripts/package.sh"
  cp "$script_dir/test-package.sh" "$repo/scripts/test-package.sh"
  chmod +x "$repo/scripts/package.sh" "$repo/scripts/test-package.sh"
  cat > "$repo/.gitignore" <<'IGNORE'
.env
.env.*
!.env.example
target/
runs/
IGNORE
  printf 'OPENAI_API_KEY=\n' > "$repo/.env.example"
  cat > "$repo/README.md" <<'README'
# Fixture
Video: https://youtu.be/abcdefghijk
README
  {
    printf '| Task | Built | Oracle 1.0 | Mutants fail | ozgur sign-off |\n'
    for task in "${tasks[@]}"; do printf '| %s | ☑ | ☑ | ☑ | ☑ |\n' "$task"; done
  } > "$repo/docs/STATUS.md"
  printf 'Task data\n' > "$repo/docs/markers.md"
  printf 'Prefix examples: sk- nvapi- ghp_ and token=EXAMPLE.\n' >> "$repo/docs/markers.md"
  printf '%s\n' '-----BEGIN PRIVATE KEY-----' >> "$repo/docs/markers.md"
  pad=$(printf '%020d' 7)
  printf 'Encrypted sample: noise_sk-%s\n' "$pad" >> "$repo/docs/markers.md"
  printf 'locked dependencies\n' > "$repo/Cargo.lock"
  printf '[workspace]\n' > "$repo/Cargo.toml"
  printf '[toolchain]\nchannel = "1.99.1"\n' > "$repo/rust-toolchain.toml"
  printf 'system prompt\n' > "$repo/prompts/system_v1.md"
  printf '{}\n' > "$repo/schemas/mandate.schema.json"
  printf '{}\n' > "$repo/schemas/offer.schema.json"
  printf '{}\n' > "$repo/schemas/final_answer.schema.json"
  for task in "${tasks[@]}"; do
    mkdir -p "$repo/tasks/$task"
    printf '{"task_id":"%s"}\n' "$task" > "$repo/tasks/$task/task.json"
    printf '{"task_id":"%s"}\n' "$task" > "$repo/reference/$task.json"
  done

  cat > "$repo/results/fixture-run/run_meta.json" <<'META'
{
  "run_id": "fixture-run",
  "kind": "run",
  "source": "provider_api",
  "state": "completed",
  "models": [{"id":"gpt-6-luna","provider":"openai"}],
  "tasks": [
    {"task_id":"t00_control","task_version":1},
    {"task_id":"t01_hidden_fees","task_version":1},
    {"task_id":"t02_expired_offer","task_version":1},
    {"task_id":"t03_compatibility","task_version":1},
    {"task_id":"t04_seller_injection","task_version":1},
    {"task_id":"t05_bundle_vs_unit","task_version":1},
    {"task_id":"t06_ship_vs_delivery","task_version":1},
    {"task_id":"t07_final_sale","task_version":1},
    {"task_id":"t08_no_valid_offer","task_version":1}
  ],
  "trials_per_task": 5
}
META
  cat > "$repo/results/fixture-run/report.json" <<'REPORT'
{
  "run_id":"fixture-run",
  "report_scope":"benchmark_results",
  "input_metadata":{"kind":"run","source":"provider_api","state":"completed"},
  "coverage":{"tasks":9,"models":1,"trials_per_task":5,"expected_trials":45,"completed_items":45}
}
REPORT
  printf 'model,trials,pass_rate\ngpt-6-luna,45,1.0\n' > "$repo/results/fixture-run/summary.csv"
  : > "$repo/results/fixture-run/scores.jsonl"
  for task in "${tasks[@]}"; do
    for trial in 1 2 3 4 5; do
      trial_id="model-0-${task}-trial-${trial}"
      printf '{"trial_id":"%s","task_id":"%s","task_version":1,"trial":%s,"provider":"openai","requested_model":"gpt-6-luna","returned_models":["gpt-6-luna"],"status":"pass","grade":{"pass":true}}\n' \
        "$trial_id" "$task" "$trial" >> "$repo/results/fixture-run/scores.jsonl"
      printf '[{"model":"gpt-6-luna"}]\n' > "$repo/results/fixture-run/outputs/$trial_id.json"
      printf '{"trial_id":"%s"}\n' "$trial_id" > "$repo/transcripts/fixture-run/$trial_id.json"
    done
  done
  mkdir -p "$repo/target/cache" "$repo/runs/scratch"
  printf 'ignored build data\n' > "$repo/target/cache/file"
  printf 'ignored run data\n' > "$repo/runs/scratch/file"
  commit_all "$repo" 'create synthetic ready packaging fixture'
  # A local secret in the ignored owner file must never be read or archived.
  local pad
  pad=$(printf '%020d' 0)
  printf 'sk-%s\n' "$pad" > "$repo/.env"
}

expect_refusal_without_archive() {
  local repo=$1 output=$2 captured
  if captured=$(cd "$repo" && ./scripts/package.sh --out "$output" 2>&1); then
    printf 'expected package refusal but command succeeded\n' >&2
    return 1
  fi
  [[ ! -e "$output" ]] || { printf 'refused package command left an archive\n' >&2; return 1; }
}

# Missing gates fail before any archive can be created.
missing="$tmp/missing"
git init -q "$missing"
git -C "$missing" config user.name 'Fixture Author'
git -C "$missing" config user.email 'fixture@example.invalid'
mkdir -p "$missing/scripts"
cp "$package_script" "$missing/scripts/package.sh"
chmod +x "$missing/scripts/package.sh"
printf '# TODO: finish\n' > "$missing/README.md"
commit_all "$missing" 'create incomplete fixture'
expect_refusal_without_archive "$missing" "$tmp/missing.zip"

# A fully synthetic ready fixture passes --check and creates a ZIP only with explicit --out.
valid="$tmp/valid"
make_valid_repo "$valid"
default_output=$(cd "$valid" && ./scripts/package.sh 2>&1) || {
  printf 'default invocation did not pass a ready fixture\n' >&2
  exit 1
}
[[ ! -e "$valid/package.zip" ]] || { printf 'default check mode created an archive\n' >&2; exit 1; }
check_output=$(cd "$valid" && ./scripts/package.sh --check 2>&1) || {
  printf 'valid fixture failed check-only mode\n' >&2
  exit 1
}
[[ ! -e "$valid/package.zip" ]] || { printf 'check-only mode created an archive\n' >&2; exit 1; }
created="$valid/package.zip"
(cd "$valid" && ./scripts/package.sh --out "$created" >/dev/null)
[[ -s "$created" ]] || { printf 'archive was not created\n' >&2; exit 1; }
if command -v unzip >/dev/null 2>&1; then
  listing=$(unzip -Z -1 "$created")
  printf '%s\n' "$listing" | rg -q '^README\.md$' || { printf 'README.md is not at ZIP root\n' >&2; exit 1; }
  while IFS= read -r member; do
    case "$member" in
      .env|.env.*|*/.env|*/.env.*)
        [[ "$member" == ".env.example" ]] || { printf 'private environment file entered the ZIP\n' >&2; exit 1; }
        ;;
    esac
    case "/$member/" in */target/*|*/runs/*) printf 'ignored build/run data entered the ZIP\n' >&2; exit 1 ;; esac
  done <<< "$listing"
fi

# A Drive file link is permitted, while a lookalike domain is refused.
drive_video="$tmp/drive-video"
make_valid_repo "$drive_video"
printf '# Fixture\nVideo: [Watch](https://drive.google.com/file/d/ABCDEFGHIJ0123456789/view)\n' > "$drive_video/README.md"
commit_all "$drive_video" 'use synthetic Drive video URL'
(cd "$drive_video" && ./scripts/package.sh --check >/dev/null)
printf '# Fixture\nVideo: https://drive.google.com.evil.invalid/file/d/ABCDEFGHIJ0123456789/view\n' > "$drive_video/README.md"
commit_all "$drive_video" 'use synthetic lookalike video URL'
expect_refusal_without_archive "$drive_video" "$tmp/lookalike-video.zip"

# Existing output paths are never overwritten.
existing="$tmp/existing.zip"
printf 'preserve me\n' > "$existing"
if output=$(cd "$valid" && ./scripts/package.sh --out "$existing" 2>&1); then
  printf 'existing output path was unexpectedly accepted\n' >&2
  exit 1
fi
[[ "$(cat "$existing")" == 'preserve me' ]] || { printf 'existing output was modified\n' >&2; exit 1; }

# A prospective outcome pass may retain a legacy format failure.
prospective="$tmp/prospective"
make_valid_repo "$prospective"
jq '.metrics_policy_version=2' "$prospective/results/fixture-run/run_meta.json" > "$tmp/prospective-meta"
cp "$tmp/prospective-meta" "$prospective/results/fixture-run/run_meta.json"
jq -c '. + {metrics_policy_version:2,outcome_pass:true,strict_pass:false,format_compliant:false,grade:{pass:false}}' \
  "$prospective/results/fixture-run/scores.jsonl" > "$tmp/prospective-scores"
cp "$tmp/prospective-scores" "$prospective/results/fixture-run/scores.jsonl"
commit_all "$prospective" 'record prospective metric fixture'
(cd "$prospective" && ./scripts/package.sh --check >/dev/null)

# Complete coverage consisting only of provider errors is not benchmark evidence.
all_errors="$tmp/all-errors"
make_valid_repo "$all_errors"
jq -c '.status="api_error" | .grade=null' "$all_errors/results/fixture-run/scores.jsonl" > "$tmp/error-scores"
cp "$tmp/error-scores" "$all_errors/results/fixture-run/scores.jsonl"
jq '.state="completed_with_api_errors"' "$all_errors/results/fixture-run/run_meta.json" > "$tmp/error-meta"
cp "$tmp/error-meta" "$all_errors/results/fixture-run/run_meta.json"
jq '.input_metadata.state="completed_with_api_errors"' "$all_errors/results/fixture-run/report.json" > "$tmp/error-report"
cp "$tmp/error-report" "$all_errors/results/fixture-run/report.json"
commit_all "$all_errors" 'record all-error coverage fixture'
expect_refusal_without_archive "$all_errors" "$tmp/all-errors.zip"

# Dirty and untracked source are refused before an archive is created.
dirty_repo="$tmp/dirty"
make_valid_repo "$dirty_repo"
printf 'untracked source\n' > "$dirty_repo/new-source.rs"
expect_refusal_without_archive "$dirty_repo" "$tmp/dirty.zip"

# A tracked private environment file is rejected from the manifest.
tracked_env_repo="$tmp/tracked-env"
make_valid_repo "$tracked_env_repo"
printf 'placeholder only\n' > "$tracked_env_repo/.env.production"
git -C "$tracked_env_repo" add -f .env.production
commit_all "$tracked_env_repo" 'add synthetic forbidden env path'
if output=$(cd "$tracked_env_repo" && ./scripts/package.sh --check 2>&1); then
  printf 'tracked environment file was not refused\n' >&2
  exit 1
fi
[[ "$output" == *'.env.production'* ]] || { printf 'tracked environment filename was not reported\n' >&2; exit 1; }

# The public environment template may list names but must keep every value blank.
template_repo="$tmp/env-template"
make_valid_repo "$template_repo"
printf 'OPENAI_API_KEY=EXAMPLE\n' > "$template_repo/.env.example"
commit_all "$template_repo" 'add synthetic nonblank template value'
expect_refusal_without_archive "$template_repo" "$tmp/env-template.zip"

# Credential files are refused by manifest even when their contents are binary or opaque.
cert_repo="$tmp/cert-file"
make_valid_repo "$cert_repo"
printf 'opaque fixture data\n' > "$cert_repo/docs/private.pem"
commit_all "$cert_repo" 'add synthetic private-key extension'
if output=$(cd "$cert_repo" && ./scripts/package.sh --check 2>&1); then
  printf 'tracked private-key extension was not refused\n' >&2
  exit 1
fi
[[ "$output" == *'docs/private.pem'* ]] || { printf 'private-key filename was not reported\n' >&2; exit 1; }

# A tracked key-like value blocks packaging and never appears in diagnostics.
tree_secret_repo="$tmp/tree-secret"
make_valid_repo "$tree_secret_repo"
pad=$(printf '%020d' 1)
secret="sk-$pad"
{
  printf 'api_key=%s\n' "$secret"
  printf '%1000000s\n' 'x'
} > "$tree_secret_repo/docs/credential.txt"
commit_all "$tree_secret_repo" 'add synthetic tracked credential'
if output=$(cd "$tree_secret_repo" && ./scripts/package.sh --check 2>&1); then
  printf 'tracked secret was not blocked\n' >&2
  exit 1
fi
[[ "$output" != *"$secret"* ]] || { printf 'tracked secret appeared in diagnostics\n' >&2; exit 1; }
[[ "$output" == *'docs/credential.txt'* ]] || { printf 'tracked secret file path was not reported\n' >&2; exit 1; }
[[ "$output" == *'in tracked file'* ]] || { printf 'tracked tree scan did not report its finding\n' >&2; exit 1; }

# Every supported marker family and a partial PEM block are caught without printing values.
marker_repo="$tmp/marker-secret"
make_valid_repo "$marker_repo"
pad=$(printf '%020d' 3)
sk_secret="sk-$pad"
nvapi_secret="nvapi-$pad"
ghp_secret="ghp_$pad"
gho_secret="gho_$pad"
akia_secret="AKIA$(printf '%016d' 0)"
bearer_secret="Bearer $pad"
api_key_secret="api_key=$pad"
token_secret="token=$pad"
key_type='PRIVATE KEY'
private_begin="-----BEGIN $key_type-----"
private_body=$(printf '%048d' 0)
{
  printf '%s\n' "$sk_secret" "$nvapi_secret" "$ghp_secret" "$gho_secret" \
    "$akia_secret" "$bearer_secret" "$api_key_secret" "$token_secret" \
    "$private_begin" "$private_body"
} > "$marker_repo/docs/credential-markers.txt"
commit_all "$marker_repo" 'add synthetic marker coverage'
if output=$(cd "$marker_repo" && ./scripts/package.sh --check 2>&1); then
  printf 'credential marker fixture was not blocked\n' >&2
  exit 1
fi
for secret in "$sk_secret" "$nvapi_secret" "$ghp_secret" "$gho_secret" "$akia_secret" \
  "$bearer_secret" "$api_key_secret" "$token_secret" "$private_body"; do
  [[ "$output" != *"$secret"* ]] || { printf 'credential value appeared in diagnostics\n' >&2; exit 1; }
done

# Both documented Meta key formats are detected without echoing the values.
meta_repo="$tmp/meta-secret"
make_valid_repo "$meta_repo"
meta_underscore="LLM_123456_$pad"
meta_pipe="LLM|123456|$pad"
printf '%s\n' "$meta_underscore" > "$meta_repo/docs/meta-underscore.txt"
printf '%s\n' "$meta_pipe" > "$meta_repo/docs/meta-pipe.txt"
commit_all "$meta_repo" 'add synthetic Meta key markers'
if output=$(cd "$meta_repo" && ./scripts/package.sh --check 2>&1); then
  printf 'Meta credential fixtures were not blocked\n' >&2
  exit 1
fi
[[ "$output" == *'credential-shaped value in tracked file: docs/meta-underscore.txt'* ]]
[[ "$output" == *'credential-shaped value in tracked file: docs/meta-pipe.txt'* ]]
[[ "$output" != *"$meta_underscore"* && "$output" != *"$meta_pipe"* ]]

# A credential removed from the current tree still blocks through history scanning.
history_secret_repo="$tmp/history-secret"
make_valid_repo "$history_secret_repo"
pad=$(printf '%020d' 2)
secret="token=$pad"
printf '%s\n' "$secret" > "$history_secret_repo/docs/old-credential.txt"
commit_all "$history_secret_repo" 'add synthetic historical credential'
git -C "$history_secret_repo" rm -q docs/old-credential.txt
git -C "$history_secret_repo" -c user.name='Fixture Author' -c user.email='fixture@example.invalid' commit -qm 'remove synthetic historical credential'
if output=$(cd "$history_secret_repo" && ./scripts/package.sh --check 2>&1); then
  printf 'historical secret was not blocked\n' >&2
  exit 1
fi
[[ "$output" != *"$secret"* ]] || { printf 'historical secret appeared in diagnostics\n' >&2; exit 1; }
[[ "$output" == *'docs/old-credential.txt'* ]] || { printf 'historical secret path was not reported\n' >&2; exit 1; }

printf 'package utility tests passed\n'
