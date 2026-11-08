# Submission readiness · Nov 8, 2026

The signed Muse diagnostic is complete and its selected evidence is committed. The owner provided the video link. Playback verification, final narrative review, clean-room reproduction and the packaging coverage decision remain open.

| Item | Current evidence | Remaining work |
|---|---|---|
| Task/grader review | Both SIGNOFF boxes, all nine STATUS owner cells and signed chat approval completed Nov 8 | Owner reviews the final narrative and archived failing transcripts |
| Offline validation | Nine reference oracles and 144 workspace tests passed in the last code verification | Final-version clean-room rerun from the README |
| Historical Luna pilot | t00/t01/t04/t08 v1 ×3, original strict 9/12. All three failures omitted currency after correct ASK_USER. | Preserve the original result and its limited coverage |
| Muse diagnostic | t01/t02/t05/t06 v2 ×3. Outcome, strict and raw format each 12/12, no decisions missing, zero API errors. | Four-task coverage must be stated plainly. [Review](MUSE_DIAGNOSTIC_V1.md) |
| Spend | Muse batch estimate USD 0.20152740. Conservative remaining ledger capacity USD 14.51772725. | Any further paid batch needs a new owner order |
| README and form text | Real coverage, settings, runtime, cost and per-task findings filled. Submission draft is 123 words. [Blurb](SUBMISSION_BLURB.md) | Owner reviews final wording |
| Video | Owner supplied [the Google Drive recording](https://drive.google.com/file/d/1VA8wZQtAJhCOLPDjF81hzegxUXQ6JuZQ/view). Drive permissions list anyone as reader. | Owner confirms playback with audio and duration at most five minutes. Use the same URL in the submission field. |
| ZIP | Read-only helper still requires finished README/video and nine-task/five-trial committed results | Current diagnostics do not meet the internal coverage rule. Owner must decide how to resolve it before a real ZIP |
| GitHub | Work is published on PR #2. M1 is merged. | PR #2 merge needs an explicit owner instruction |
| Final upload | Agreement was accepted by owner | Owner performs the final ZIP, video URL and short-description upload |

## Coverage and packaging

The selected Muse evidence covers four of the nine designed tasks. The archived Luna pilot uses a different task set and version-1 inputs, so its 9/12 strict score is historical context rather than a matched comparison.

The owner may choose a clearly labeled diagnostic submission, but the current package helper's nine-task/five-trial rule still applies. Relaxing that internal rule needs an explicit project decision. A later nine-task/three-trial batch would also need a new paid work order and a fresh budget projection.

Officially permitted video hosts include YouTube, Loom, Google Drive, Vimeo or similar. The packaging helper now recognizes YouTube and Google Drive file links. [Captured requirements](HACKATHON.md)

## Owner finish sequence

Review the final results and narrative, record the video, and add the same accessible video URL to README and the submission field. Resolve the internal evidence-coverage gate before invoking the real ZIP command. After the ZIP passes size, secret and archive checks, the owner uploads ZIP + video URL + description.

The deadline is **Nov 9, 2026, 08:15 Istanbul / 05:15 GMT**. A second paid run, video publication, real ZIP, merge and final upload are outside the completed diagnostic work.

The clean post-diagnostic `scripts/package.sh --check` reports three pending gates: the video TODO in README, the video URL and the committed nine-task/five-trial coverage requirement. No real ZIP was created.

## Video link provided · Nov 8

The owner supplied [the final video URL](https://drive.google.com/file/d/1VA8wZQtAJhCOLPDjF81hzegxUXQ6JuZQ/view). Drive metadata identifies an MP4 recording and lists an anyone/reader permission. Duration and audio playback still need owner confirmation. The README video placeholder is replaced, and the same URL should be used in the submission field. The results-coverage requirement remains in force.

The clean packaging check after adding the video reports one remaining gate: committed results covering nine tasks with at least five trials each. The README marker and video-URL gates pass.
