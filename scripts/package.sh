#!/usr/bin/env bash
set -euo pipefail

# Maintainer-only preflight/packager. jq validates result metadata; it is not an app runtime dependency.
usage() {
  cat <<'USAGE'
Usage: scripts/package.sh [--check | --out PATH.zip]

With no arguments (or --check), run a read-only preflight and create no archive.
--out PATH.zip explicitly creates a git-archive ZIP after every gate passes.
Maintainer prerequisite: jq, for validating committed result coverage metadata.
USAGE
}

mode=check
out_arg=
if (($# == 0)); then
  :
else
  case "$1" in
    --help|-h) usage; exit 0 ;;
    --check) [[ $# == 1 ]] || { usage >&2; exit 2; } ;;
    --out)
      [[ $# == 2 && -n "$2" ]] || { usage >&2; exit 2; }
      mode=archive
      out_arg=$2
      ;;
    *) usage >&2; exit 2 ;;
  esac
fi

root=$(git rev-parse --show-toplevel 2>/dev/null) || {
  printf 'Run this from a mandate-bench Git checkout.\n' >&2
  exit 2
}
cd "$root"

failures=0
blocked() {
  printf 'BLOCKED: %s\n' "$1"
  failures=$((failures + 1))
}

if ! command -v rg >/dev/null 2>&1; then
  blocked "ripgrep (rg) is required for safe marker scanning"
fi
if ! command -v jq >/dev/null 2>&1; then
  blocked "jq is required to validate run metadata and score coverage (maintainer-only; not an app dependency)"
fi

status=$(git status --porcelain --untracked-files=all)
if [[ -n "$status" ]]; then
  blocked "source checkout has tracked changes or untracked source files; commit the selected source first"
fi

# Do not archive paths that could contain local credentials, generated output, media, or archives.
tracked_env_example=0
while IFS= read -r -d '' entry; do
  mode_field=${entry%% *}
  path=${entry#*$'\t'}
  base=${path##*/}
  lower_path=$(printf '%s' "$path" | tr '[:upper:]' '[:lower:]')
  if [[ "$mode_field" == "120000" ]]; then
    blocked "tracked symlink is not allowed in the submission manifest: $path"
  fi
  if [[ "$path" == ".env.example" ]]; then
    tracked_env_example=1
  elif [[ "$base" == ".env" || "$base" == .env.* ]]; then
    blocked "private environment file is tracked: $path"
  fi
  case "/$lower_path/" in
    */target/*|*/runs/*|*/videos/*|*/video/*|*/archives/*|*/archive/*|*/results/_scratch/*|*/transcripts/_scratch/*|*/secrets/*|*/secret/*)
      blocked "generated or non-submission directory is tracked: $path"
      ;;
  esac
  case "$lower_path" in
    *.key|*.pem|*.p12|*.pfx|*.crt|*.der|*.jks|*.keystore|*.secret|*.secrets|.netrc|*/.netrc|.npmrc|*/.npmrc|.pypirc|*/.pypirc|id_rsa*|*/id_rsa*|id_ed25519*|*/id_ed25519*|credentials*.json|*/credentials*.json|service-account*.json|*/service-account*.json)
      blocked "private key, certificate, or credential file is tracked: $path"
      ;;
  esac
  case "$lower_path" in
    *.mp4|*.m4v|*.mov|*.webm|*.mkv|*.avi|*.mp3|*.m4a|*.wav|*.zip|*.tar|*.tar.gz|*.tgz|*.tar.xz|*.tar.bz2|*.7z|*.rar|*.gz|*.bz2|*.xz)
      blocked "video or archive file is tracked: $path"
      ;;
  esac
done < <(git ls-files --stage -z)
[[ "$tracked_env_example" == 1 ]] || blocked ".env.example must be tracked and contain names only"
if [[ -f .env.example && ! -L .env.example ]]; then
  while IFS= read -r line || [[ -n "$line" ]]; do
    trimmed=${line#"${line%%[![:space:]]*}"}
    [[ -z "$trimmed" || "$trimmed" == \#* ]] && continue
    [[ "$trimmed" =~ ^[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=[[:space:]]*$ ]] || {
      blocked ".env.example must contain environment variable names with blank values only"
      break
    }
  done < .env.example
fi

required_files=(README.md Cargo.toml Cargo.lock rust-toolchain.toml .env.example docs/STATUS.md)
for path in "${required_files[@]}"; do
  git ls-files --error-unmatch "$path" >/dev/null 2>&1 || blocked "required tracked file is missing: $path"
done

tasks=(t00_control t01_hidden_fees t02_expired_offer t03_compatibility t04_seller_injection t05_bundle_vs_unit t06_ship_vs_delivery t07_final_sale t08_no_valid_offer)
for task in "${tasks[@]}"; do
  for path in "tasks/$task/task.json" "reference/$task.json"; do
    git ls-files --error-unmatch "$path" >/dev/null 2>&1 || blocked "required tracked task/reference is missing: $path"
  done
done

if rg -q -i '\b(TODO|TBD|PLACEHOLDER)\b' README.md; then
  blocked "README.md still contains unresolved TODO/TBD/PLACEHOLDER markers"
fi
if ! rg -q 'https://(www\.)?youtube\.com/watch\?v=[A-Za-z0-9_-]{11}([^A-Za-z0-9_-]|$)|https://youtu\.be/[A-Za-z0-9_-]{11}([^A-Za-z0-9_-]|$)' README.md; then
  blocked "README.md needs an actual YouTube watch or youtu.be URL"
fi

for task in "${tasks[@]}"; do
  rows=$(rg --count --fixed-strings "| $task |" docs/STATUS.md || true)
  if [[ "$rows" != 1 ]]; then
    blocked "docs/STATUS.md must contain exactly one current sign-off row for $task"
    continue
  fi
  row=$(rg --fixed-strings "| $task |" docs/STATUS.md | head -n 1)
  signoff=$(printf '%s\n' "$row" | awk -F'|' '{gsub(/^[[:space:]]+|[[:space:]]+$/, "", $6); print $6}')
  [[ "$signoff" == "☑" || "$signoff" == "✅" ]] || blocked "owner sign-off is not checked for $task in docs/STATUS.md"
done

# Credential patterns are boundary-anchored and length-bounded so docs mentioning only prefixes do not trip.
# Matches are held only in shell variables; all diagnostics print file names and commit paths, never match text.
secret_re="(^|[^[:alnum:]_-])([sS][kK]-[[:alnum:]][[:alnum:]_-]{15,}|[nN][vV][aA][pP][iI]-[[:alnum:]][[:alnum:]_-]{15,}|[gG][hH][pP]_[[:alnum:]]{20,}|[gG][hH][oO]_[[:alnum:]]{20,}|[aA][kK][iI][aA][[:upper:][:digit:]]{16}|[bB][eE][aA][rR][eE][rR][[:space:]]+[[:alnum:]._~+/=-]{16,}|([aA][pP][iI][_ -]?[kK][eE][yY]|[tT][oO][kK][eE][nN])[[:space:]]*[:=][[:space:]]*[\"']?[[:alnum:]._~+/=-]{16,})"
private_begin='-----BEGIN [A-Z ]*PRIVATE KEY-----'

blob_has_secret() {
  local blob=$1
  if git show "$blob" 2>/dev/null | rg --text --count-matches --regexp "$secret_re" >/dev/null; then
    return 0
  fi
  if git show "$blob" 2>/dev/null | rg --text --count-matches --regexp "$private_begin" >/dev/null \
    && git show "$blob" 2>/dev/null | rg --text --count-matches '^[A-Za-z0-9+/]{40,}={0,2}$' >/dev/null; then
    return 0
  fi
  return 1
}

# Build the tracked-file scan list without ever opening .env/.env.* files (except the public template).
safe_files=()
while IFS= read -r -d '' path; do
  base=${path##*/}
  if [[ "$base" == ".env" || ( "$base" == .env.* && "$path" != ".env.example" ) ]]; then
    continue
  fi
  safe_files+=("$path")
done < <(git ls-files -z)

secret_found=0
while IFS= read -r -d '' path; do
  path=${path#HEAD:}
  if blob_has_secret "HEAD:$path"; then
    printf 'BLOCKED: credential-shaped value in tracked file: %s\n' "$path"
    secret_found=1
  fi
done < <(git grep --text --null --files-with-matches --extended-regexp "$secret_re|$private_begin" HEAD -- "${safe_files[@]}" 2>/dev/null || true)

history_hits=$(git log --all --root --text --extended-regexp --regexp-ignore-case \
  -G "$secret_re|$private_begin" --format='commit %H' --name-only -- . 2>/dev/null || true)
history_found=0
current_commit=
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  if [[ "$line" == commit\ * ]]; then
    current_commit=${line#commit }
    continue
  fi
  [[ -n "$current_commit" ]] || continue
  case "${line##*/}" in
    .env) printf 'BLOCKED: private environment file appears in Git history: %s:%s\n' "$current_commit" "$line"; history_found=1; continue ;;
    .env.*)
      [[ "$line" == ".env.example" ]] || { printf 'BLOCKED: private environment file appears in Git history: %s:%s\n' "$current_commit" "$line"; history_found=1; continue; }
      ;;
  esac
  if blob_has_secret "$current_commit:$line" || blob_has_secret "$current_commit^:$line"; then
    printf 'BLOCKED: credential-shaped value in Git history: %s:%s\n' "$current_commit" "$line"
    history_found=1
  fi
done <<< "$history_hits"
if [[ "$history_found" == 1 || "$secret_found" == 1 ]]; then
  failures=$((failures + 1))
fi

validate_run() {
  local meta_path=$1 run_dir=${1%/run_meta.json} run_id=${1#results/} transcript_dir="transcripts/${1#results/}" \
    tasks_json='["t00_control","t01_hidden_fees","t02_expired_offer","t03_compatibility","t04_seller_injection","t05_bundle_vs_unit","t06_ship_vs_delivery","t07_final_sale","t08_no_valid_offer"]'
  run_id=${run_id%/run_meta.json}
  transcript_dir=${transcript_dir%/run_meta.json}
  local meta="$meta_path" report="$run_dir/report.json" summary="$run_dir/summary.csv" scores="$run_dir/scores.jsonl"
  local trials model_count model_ids expected trial_id raw transcript

  jq -e --arg run_id "$run_id" --argjson task_ids "$tasks_json" '
    .kind == "run" and .source == "provider_api" and
    (.state == "completed" or .state == "completed_with_api_errors") and
    .run_id == $run_id and
    (.tasks | type == "array" and (map(.task_id) | sort == $task_ids)) and
    (.models | type == "array" and length >= 1 and length <= 20 and all(.[]; (.id | type == "string" and length > 0))) and
    (.trials_per_task | type == "number" and . >= 5 and . <= 1000)
  ' "$meta" >/dev/null 2>&1 || return 1

  trials=$(jq -r '.trials_per_task' "$meta")
  model_count=$(jq -r '.models | length' "$meta")
  model_ids=$(jq -c '[.models[].id] | sort' "$meta")
  expected=$((9 * trials * model_count))

  [[ -f "$report" && -f "$summary" && -f "$scores" ]] || return 1
  [[ -s "$summary" ]] || return 1
  jq -e --arg run_id "$run_id" --arg state "$(jq -r '.state' "$meta")" \
    --argjson trials "$trials" --argjson expected "$expected" '
    .run_id == $run_id and .report_scope == "benchmark_results" and
    .input_metadata.kind == "run" and .input_metadata.source == "provider_api" and
    .input_metadata.state == $state and
    .coverage.tasks == 9 and .coverage.trials_per_task == $trials and
    .coverage.expected_trials == $expected and .coverage.completed_items == $expected
  ' "$report" >/dev/null 2>&1 || return 1

  jq -s -e --argjson expected "$expected" --argjson trials "$trials" \
    --argjson task_ids "$tasks_json" --argjson model_ids "$model_ids" \
    --arg state "$(jq -r '.state' "$meta")" '
      def key: "\(.requested_model)\u0000\(.task_id)\u0000\(.trial)";
      ([.[] | select(.status == "api_error")] | length) as $api_errors |
      length == $expected and
      (if $api_errors == 0 then $state == "completed" else $state == "completed_with_api_errors" end) and
      ([.[].trial_id] | unique | length) == $expected and
      ([.[].task_id] | unique | sort) == $task_ids and
      ([.[].requested_model] | unique | sort) == $model_ids and
      ([.[] | key] | unique | length) == $expected and
      all(.[]; (.trial_id | type == "string" and test("^[A-Za-z0-9._-]+$")) and
        (.status == "pass" or .status == "fail" or .status == "api_error") and
        (if .status == "api_error" then .grade == null else .grade.pass == (.status == "pass") end) and
        (.trial | type == "number" and . >= 1 and . <= $trials))
    ' "$scores" >/dev/null 2>&1 || return 1
  [[ "$expected" -ge 45 ]] || return 1

  while IFS= read -r trial_id; do
    raw="$run_dir/outputs/$trial_id.json"
    transcript="$transcript_dir/$trial_id.json"
    [[ -f "$raw" && -f "$transcript" ]] || return 1
    git ls-files --error-unmatch "$raw" "$transcript" >/dev/null 2>&1 || return 1
  done < <(jq -r '.trial_id' "$scores")
  git ls-files --error-unmatch "$meta_path" "$report" "$summary" "$scores" >/dev/null 2>&1 || return 1
}

if ! command -v jq >/dev/null 2>&1; then
  : # The missing-jq gate was already recorded above.
else
  found_run=0
  while IFS= read -r meta_path; do
    [[ -n "$meta_path" ]] || continue
    if validate_run "$meta_path"; then
      found_run=1
      break
    fi
  done < <(git ls-files 'results/*/run_meta.json')
  [[ "$found_run" == 1 ]] || blocked "no committed results run has provider_api metadata, 9 tasks, at least 5 trials/task, report outputs, raw payloads, and matching transcripts"
fi

if ((failures > 0)); then
  printf 'Package preflight failed: %s gate(s) pending. No archive created.\n' "$failures" >&2
  exit 1
fi

if [[ "$mode" == check ]]; then
  printf 'Package preflight passed. Check-only mode created no archive.\n'
  exit 0
fi

[[ "$out_arg" == *.zip ]] || { printf 'Output path must end in .zip.\n' >&2; exit 2; }
out_dir=$(dirname "$out_arg")
out_name=$(basename "$out_arg")
[[ "$out_dir" == /* ]] || out_dir="$root/$out_dir"
[[ -d "$out_dir" ]] || { printf 'Output directory does not exist.\n' >&2; exit 2; }
out_dir=$(cd "$out_dir" && pwd -P)
out_path="$out_dir/$out_name"
[[ ! -e "$out_path" && ! -L "$out_path" ]] || { printf 'Refusing to overwrite existing output: %s\n' "$out_path" >&2; exit 2; }

created_archive=0
cleanup_archive() {
  local status=$?
  trap - EXIT
  if [[ "$created_archive" == 1 && "$status" != 0 ]]; then
    rm -f "$out_path"
  fi
  exit "$status"
}
if ! (set -o noclobber; : > "$out_path") 2>/dev/null; then
  printf 'Refusing to overwrite output created concurrently: %s\n' "$out_path" >&2
  exit 2
fi
created_archive=1
trap cleanup_archive EXIT
git archive --format=zip HEAD > "$out_path"
size=$(wc -c < "$out_path" | tr -d '[:space:]')
if ((size >= 50000000)); then
  printf 'Archive is %s bytes; the limit is under 50000000 bytes. Removing only this new archive.\n' "$size" >&2
  exit 1
fi
created_archive=0
trap - EXIT
printf 'Archive created: %s (%s bytes). README.md is at ZIP root.\n' "$out_path" "$size"
