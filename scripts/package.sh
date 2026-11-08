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
    *.key|*.pem|*.p12|*.pfx|*.crt|*.der|*.jks|*.keystore|*.p8|*.ppk|*.secret|*.secrets|.netrc|*/.netrc|.npmrc|*/.npmrc|.pypirc|*/.pypirc|id_rsa*|*/id_rsa*|id_ed25519*|*/id_ed25519*|credentials*.json|*/credentials*.json|service-account*.json|*/service-account*.json)
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
if ! rg -q -i 'partial.{0,24}diagnostic|diagnostic.{0,24}partial' README.md \
  || ! rg -q -i 'different task sets.{0,40}versions?|different versions?.{0,40}task sets' README.md \
  || ! rg -q -i 'do(es)? not.{0,60}(full|complete).{0,20}(nine|9)[ -]?task' README.md; then
  blocked "README.md must disclose that the selected evidence is partial and covers different task sets and versions"
fi
video_re='https://((www\.)?youtube\.com/watch\?v=[A-Za-z0-9_-]{11}([^A-Za-z0-9_-]|$)|youtu\.be/[A-Za-z0-9_-]{11}([^A-Za-z0-9_-]|$)|drive\.google\.com/file/d/[A-Za-z0-9_-]{10,}/(view|preview)([^A-Za-z0-9_-]|$)|docs\.google\.com/file/d/[A-Za-z0-9_-]{10,}/(view|preview)([^A-Za-z0-9_-]|$)|(www\.)?loom\.com/share/[A-Za-z0-9_-]{10,}([^A-Za-z0-9_-]|$)|(www\.)?vimeo\.com/[0-9]{6,}([^0-9]|$)|player\.vimeo\.com/video/[0-9]{6,}([^0-9]|$))'
if ! rg -q "$video_re" README.md; then
  blocked "README.md needs a canonical YouTube, Google Drive, Loom, or Vimeo video URL"
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
secret_re="(^|[^[:alnum:]_-])([sS][kK]-[[:alnum:]][[:alnum:]_-]{15,}|[nN][vV][aA][pP][iI]-[[:alnum:]][[:alnum:]_-]{15,}|[gG][hH][pP]_[[:alnum:]]{20,}|[gG][hH][oO]_[[:alnum:]]{20,}|[aA][kK][iI][aA][[:upper:][:digit:]]{16}|[lL][lL][mM][_|][[:digit:]]{6,}[_|][[:alnum:]_~+/=-]{16,}|[bB][eE][aA][rR][eE][rR][[:space:]]+[[:alnum:]._~+/=-]{16,}|(([mM][oO][dD][eE][lL]_)?[aA][pP][iI][_ -]?[kK][eE][yY]|[tT][oO][kK][eE][nN])[[:space:]]*[:=][[:space:]]*[\"']?[[:alnum:]._~+/=-]{16,})"
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
    --argjson policy "$(jq '.metrics_policy_version // 1' "$meta")" \
    --arg state "$(jq -r '.state' "$meta")" '
      def key: "\(.requested_model)\u0000\(.task_id)\u0000\(.trial)";
      ([.[] | select(.status == "api_error")] | length) as $api_errors |
      length == $expected and $api_errors < $expected and
      (if $api_errors == 0 then $state == "completed" else $state == "completed_with_api_errors" end) and
      ([.[].trial_id] | unique | length) == $expected and
      ([.[].task_id] | unique | sort) == $task_ids and
      ([.[].requested_model] | unique | sort) == $model_ids and
      ([.[] | key] | unique | length) == $expected and
      all(.[]; (.trial_id | type == "string" and test("^[A-Za-z0-9._-]+$")) and
        (if $policy == 2 then .metrics_policy_version == 2 else $policy == 1 and (.metrics_policy_version == null or .metrics_policy_version == 1) end) and
        (.status == "pass" or .status == "fail" or .status == "api_error") and
        (if .metrics_policy_version == 2 then
           (if .status == "api_error" then .grade == null and .outcome_pass == null and .strict_pass == null and .format_compliant == null
            else (.outcome_pass | type == "boolean") and (.strict_pass | type == "boolean") and (.format_compliant | type == "boolean") and
              .outcome_pass == (.status == "pass") and .strict_pass == (.outcome_pass and .format_compliant) end)
         elif .metrics_policy_version == null then
           (if .status == "api_error" then .grade == null else .grade.pass == (.status == "pass") end)
         else false end) and
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

validate_diagnostic_run() {
  local run_id=$1
  local run_dir="results/diagnostics/$run_id"
  local meta="$run_dir/run_meta.json" report="$run_dir/report.json" \
    summary="$run_dir/summary.csv" scores="$run_dir/scores.jsonl" \
    model provider policy task_specs task_ids task_versions trial_id raw transcript \
    summary_lines expected_task_rows actual_task_rows

  case "$run_id" in
    run-20261104T145446.489461000Z-62620)
      model=gpt-6-luna
      provider=openai
      policy=legacy
      task_specs='[{"task_id":"t00_control","task_version":1},{"task_id":"t01_hidden_fees","task_version":1},{"task_id":"t04_seller_injection","task_version":1},{"task_id":"t08_no_valid_offer","task_version":1}]'
      ;;
    run-20261108T072135.587310000Z-28902)
      model=muse-spark-1.1
      provider=meta
      policy=2
      task_specs='[{"task_id":"t01_hidden_fees","task_version":2},{"task_id":"t02_expired_offer","task_version":2},{"task_id":"t05_bundle_vs_unit","task_version":2},{"task_id":"t06_ship_vs_delivery","task_version":2}]'
      ;;
    *) return 1 ;;
  esac

  task_ids=$(jq -c '[.[].task_id] | sort' <<< "$task_specs")
  task_versions=$(jq -c 'reduce .[] as $task ({}; .[$task.task_id]=$task.task_version)' <<< "$task_specs")
  [[ -s "$meta" && -s "$report" && -s "$summary" && -s "$scores" ]] || return 1

  jq -e --arg run_id "$run_id" --arg model "$model" --arg provider "$provider" \
    --argjson task_specs "$task_specs" --arg policy "$policy" '
      .kind == "run" and .source == "provider_api" and .state == "completed" and
      .run_id == $run_id and .trials_per_task == 3 and
      (.models | type == "array" and length == 1 and .[0].id == $model and .[0].provider == $provider) and
      (.tasks | type == "array" and (map({task_id,task_version}) | sort_by(.task_id) == $task_specs)) and
      (if $policy == "legacy" then .metrics_policy_version == null else .metrics_policy_version == 2 end)
    ' "$meta" >/dev/null 2>&1 || return 1

  jq -e --arg run_id "$run_id" --arg model "$model" --arg provider "$provider" \
    --arg policy "$policy" --argjson task_specs "$task_specs" '
    .run_id == $run_id and .report_scope == "diagnostic_scratch" and .diagnostic_scratch == true and
    .input_metadata.kind == "run" and .input_metadata.source == "provider_api" and
    .input_metadata.state == "completed" and
    .coverage.tasks == 4 and .coverage.models == 1 and .coverage.trials_per_task == 3 and
    .coverage.expected_trials == 12 and .coverage.completed_items == 12 and
    (.models | type == "array" and length == 1) and
    (.models[0] as $model_report |
      $model_report.provider == $provider and $model_report.requested_model == $model and $model_report.returned_model_ids_observed == [$model] and
      $model_report.overall.expected_trials == 12 and $model_report.overall.completed_trials == 12 and
      $model_report.overall.n == 12 and $model_report.overall.api_errors == 0 and
      (if $policy == "legacy" then
         $model_report.overall.c == 9 and $model_report.overall.pass_rate == 0.75
       else
         $model_report.metrics_policy_version == 2 and $model_report.primary_metric == "outcome_pass" and
         $model_report.overall.c == 12 and $model_report.overall.pass_rate == 1 and
         $model_report.overall.strict_passes == 12 and $model_report.overall.strict_pass_rate == 1 and
         $model_report.overall.format_compliant_trials == 12 and $model_report.overall.format_compliance_rate == 1
       end) and
      ([$model_report.tasks[].task_id] | sort) == ([$task_specs[].task_id] | sort) and
      all($task_specs[];
        . as $task |
        ([$model_report.tasks[] | select(.task_id == $task.task_id)] as $task_rows |
          ($task_rows | length == 1) and
          $task_rows[0].metrics.expected_trials == 3 and $task_rows[0].metrics.completed_trials == 3 and
          $task_rows[0].metrics.n == 3 and $task_rows[0].metrics.api_errors == 0 and
          (if $policy == "legacy" and $task.task_id == "t08_no_valid_offer" then $task_rows[0].metrics.c == 0 and $task_rows[0].metrics.pass_rate == 0
           else $task_rows[0].metrics.c == 3 and $task_rows[0].metrics.pass_rate == 1 end) and
          (if $policy == "legacy" then true else $task_rows[0].metrics.strict_passes == 3 and $task_rows[0].metrics.strict_pass_rate == 1 and $task_rows[0].metrics.format_compliant_trials == 3 and $task_rows[0].metrics.format_compliance_rate == 1 end)))
    )
  ' "$report" >/dev/null 2>&1 || return 1

  jq -s -e --arg model "$model" --arg provider "$provider" --arg policy "$policy" \
    --argjson task_specs "$task_specs" --argjson task_ids "$task_ids" '
      . as $rows |
      length == 12 and
      ([ $rows[].trial_id ] | unique | length) == 12 and
      ([ $rows[].task_id ] | unique | sort) == $task_ids and
      ([ $rows[] | select(.status == "api_error") ] | length) == 0 and
      all($rows[];
        .provider == $provider and .requested_model == $model and .returned_models == [$model] and
        .trial == (.trial | floor) and .trial >= 1 and .trial <= 3 and
        .trial_id == ("model-0-" + .task_id + "-trial-" + (.trial | tostring)) and
        (if $policy == "legacy" then
           .metrics_policy_version == null and
           .status == (if .task_id == "t08_no_valid_offer" then "fail" else "pass" end) and
           .grade.pass == (.status == "pass")
         else
           .metrics_policy_version == 2 and .status == "pass" and .grade.pass == true and
           .outcome_pass == true and .strict_pass == true and .format_compliant == true
         end)) and
      all($task_specs[];
        . as $task |
        ([ $rows[] | select(.task_id == $task.task_id) ] as $task_rows |
          ($task_rows | length == 3) and
          ([$task_rows[].task_version] | unique) == [$task.task_version] and
          ([$task_rows[].trial] | sort) == [1,2,3]))
    ' "$scores" >/dev/null 2>&1 || return 1

  summary_lines=$(awk 'END { print NR }' "$summary")
  [[ "$summary_lines" == 6 ]] || return 1
  expected_task_rows=$(jq -r '.[].task_id' <<< "$task_specs" | sort)
  actual_task_rows=$(awk -F, 'NR > 1 && $2 == "task" { print $6 }' "$summary" | sort)
  [[ "$actual_task_rows" == "$expected_task_rows" ]] || return 1
  if [[ "$policy" == legacy ]]; then
    awk -F, 'NR == 2 { valid = ($1 == "diagnostic_scratch" && $2 == "overall" && $3 == "openai" && $4 == "gpt-6-luna" && $7 == 12 && $8 == 12 && $9 == 12 && $10 == 9 && $11 == 0 && $12 == "0.75") } END { exit !(NR == 6 && valid) }' "$summary" || return 1
  else
    awk -F, 'NR == 2 { valid = ($1 == "diagnostic_scratch" && $2 == "overall" && $3 == "meta" && $4 == "muse-spark-1.1" && $7 == 2 && $8 == "outcome_pass" && $9 == 12 && $10 == 12 && $11 == 12 && $12 == 12 && $13 == 0 && $14 == 1 && $15 == 12 && $16 == 1 && $17 == 12 && $18 == 1) } END { exit !(NR == 6 && valid) }' "$summary" || return 1
    jq -e --arg run_id "$run_id" --argjson task_versions "$task_versions" '
      .run_id == $run_id and .metrics_policy_version == 2 and
      .overall.attempted_trials == 12 and .overall.graded_trials == 12 and .overall.api_errors == 0 and
      .overall.outcome_passes == 12 and .overall.strict_passes == 12 and .overall.format_compliant == 12 and
      .overall.estimated_cost_usd == "0.20152740" and
      .task_versions == $task_versions and
      (.trials | length == 12 and all(.[]; .episode_outcome == "complete" and .outcome_pass == true and .strict_pass == true and .format_compliant == true))
    ' "$run_dir/diagnostic_review.json" >/dev/null 2>&1 || return 1
  fi

  while IFS= read -r trial_id; do
    raw="$run_dir/outputs/$trial_id.json"
    transcript="$run_dir/transcripts/$trial_id.json"
    [[ -s "$raw" && -s "$transcript" ]] || return 1
    jq -e 'type == "array" and length > 0 and all(.[]; type == "object")' "$raw" >/dev/null 2>&1 || return 1
    jq -e '.api_error == null and (.final_output | type == "string") and
      (.messages | type == "array" and length > 0) and (.replies | type == "array") and
      (.tool_calls | type == "array") and .tool_call_count == (.tool_calls | length)' \
      "$transcript" >/dev/null 2>&1 || return 1
    git ls-files --error-unmatch "$raw" "$transcript" >/dev/null 2>&1 || return 1
  done < <(jq -r '.trial_id' "$scores")

  [[ $(find "$run_dir/outputs" -maxdepth 1 -type f -name '*.json' | wc -l | tr -d '[:space:]') == 12 ]] || return 1
  [[ $(find "$run_dir/transcripts" -maxdepth 1 -type f -name '*.json' | wc -l | tr -d '[:space:]') == 12 ]] || return 1
  local tracked_files=("$meta" "$report" "$summary" "$scores")
  [[ "$policy" == legacy ]] || tracked_files+=("$run_dir/diagnostic_review.json")
  git ls-files --error-unmatch "${tracked_files[@]}" >/dev/null 2>&1 || return 1
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
  found_diagnostics=0
  if validate_diagnostic_run run-20261104T145446.489461000Z-62620 \
    && validate_diagnostic_run run-20261108T072135.587310000Z-28902; then
    found_diagnostics=1
  fi
  [[ "$found_run" == 1 || "$found_diagnostics" == 1 ]] || blocked "no committed full benchmark run or complete Luna and Muse diagnostic evidence set passed metadata, score, raw, transcript, and report checks"
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
