# Changelog

[中文](CHANGELOG.md)

All notable changes to Prudentia should be recorded here. Add the newest entry at the top of the current release section.

## Unreleased

- Added repository name explanation, vision, ideal goals, and planned capabilities to the README.
- Split bilingual documentation into separate files: Simplified Chinese stays in `.md`, and English uses `.en.md`.
- Made the changelog available in English and Simplified Chinese.
- Added Simplified Chinese alongside English throughout the README.
- Documented changelog ordering: newest entries should be inserted at the top of the current release section.
- Documented the development completion rule: update this changelog after every development change and update the README when setup, capabilities, public interfaces, or workflows change.
- Added engineering style guidance covering readability, maintainability, explainability, Rust design practices, generics, enums, traits, comments, and review expectations.
- Added Codex CLI support as the first CLI AI backend, including device-code login guidance through `codex login --device-auth`.
- Added provider-based AI and market data boundaries, including mock providers, OpenAI-compatible AI, Alpha Vantage-compatible market data, and a reusable CLI AI provider layer.
- Scaffolded the local-first Prudentia workspace with Rust backend, SQLite persistence, React + Vite frontend, portfolio import, memo workflows, investment system editing, profile feedback, and bilingual UI.
