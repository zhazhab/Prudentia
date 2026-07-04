# Agent Notes

## Development Constraints

- Start every new development task in a fresh git worktree before editing files, unless the user explicitly asks to continue in the current worktree or the current directory is already the task-specific worktree.
- During implementation checkpoints, run the smallest relevant verification for the change, such as a focused backend test, frontend unit test, or reproduction script. Avoid running heavy whole-project checks after every small edit.
- Reserve `cargo fmt --check`, `make check-backend-size`, and `make check-backend-clippy` for final merge/publish readiness. If CI already runs the equivalent checks, local execution can be skipped during normal iteration, but the final handoff or PR must mention whether CI or local checks cover them.
- Before final merge/publish, run or confirm the full required verification for the touched scope. Backend readiness normally includes `cargo test -p prudentia-backend` plus formatting, backend file-length, and clippy coverage. Frontend readiness normally includes `npm --prefix frontend test` and `npm --prefix frontend run build`.
- After every code, configuration, UX, API, workflow, or architecture change, decide whether project documentation must change too. Update the relevant docs in the same task when behavior, setup, capabilities, public interfaces, workflows, architecture boundaries, or development rules change.
- Keep paired documentation in sync: Simplified Chinese uses `.md`, and English uses `.en.md`.
- Update `CHANGELOG.md` and `CHANGELOG.en.md` for every development change, with the newest entry at the top of the current release section.
- Update `README.md` and `README.en.md` when setup steps, startup commands, environment variables, user-facing capabilities, or common workflows change.
- Update domain docs such as `docs/api.md`, `docs/architecture.md`, `docs/engineering-style.md`, and `docs/portfolio-import.md` when the corresponding contracts or rules change.
- After finishing an implementation or adjustment, start both backend and frontend with `make start`, wait until the backend is healthy, and keep the services running for user acceptance testing.
- Only create commits, push branches, or open PRs after the user explicitly asks for that step, normally after acceptance testing succeeds.

## Default Completion Flow

After every implementation, bug fix, frontend/backend adjustment, or project configuration change:

1. Run the verification commands that match the change scope.
2. Check whether `CHANGELOG`, `README`, API docs, architecture docs, engineering rules, or feature-specific docs need updates, and make those updates before handoff.
3. Stop any old local dev server if it is still running.
4. Start the app with `make start`.
5. Wait until the backend is listening.
6. Check `http://127.0.0.1:8080/health` or the actual backend URL printed by `make start`.
7. Report the frontend URL, backend URL, documentation updates, and verification commands/results to the user.

If the user explicitly says not to run tests, not to start the app, or to only analyze, follow the newest user instruction.

## Fast Interaction Defaults

- Treat "实现并启动" as: implement, verify, restart with `make start`, then report URLs.
- Treat "只分析" as: inspect and explain without editing files.
- Treat "先别测" as: skip long verification and run only the smallest useful check.
- Treat "提交并 push" as: inspect the diff, commit intentionally, and push the current branch.
- Treat "截图回归" as: run the portfolio image import path with the provided screenshot and summarize the extracted rows.
