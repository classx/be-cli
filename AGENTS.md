<!-- mem-cli:start -->
## mem-cli context storage

Project context is stored locally per developer, outside the repository:
`${XDG_DATA_HOME:-~/.local/share}/mem/<slug>/project_context.db`.
The project slug (`bbe-cli-eee6a9849e6751fd`) is fixed in the `.mem-project` file.
The path can be overridden via the `MEMORY_DB_DIR` variable.
<!-- mem-cli:end -->

## RFC workflow (`rfc-cli`)

For RFC lifecycle operations, use `rfc-cli` commands instead of manual index edits.

- Change status: `rfc-cli set <NUMBER> <STATUS>`
- Update links to code: `rfc-cli link <NUMBER> <PATH>` / `rfc-cli unlink <NUMBER> <PATH>`
- Update dependencies: `rfc-cli edit <NUMBER>` (frontmatter), then run `rfc-cli reindex` and `rfc-cli check <NUMBER>`

Validation helpers:
- `rfc-cli status <NUMBER>`
- `rfc-cli deps <NUMBER>`
- `rfc-cli check <NUMBER>`

Do **not** manually edit `docs/rfcs/.index.json`; regenerate it via `rfc-cli reindex` when needed.

### Common pitfalls

- If `rfc-cli` reports an index parse error (for example: `Failed to parse index file`), do not fix `.index.json` manually.
- Recovery flow:
  1. Ensure RFC frontmatter is valid (`status`, `dependencies`, `links`, etc.).
  2. Run `rfc-cli reindex`.
  3. Run `rfc-cli check <NUMBER>` (or `rfc-cli check` for all RFCs).

Перед каждым commit запускать тесты. Перед merge в `main`: build + test + lint.

## Git workflow

- Каждая фаза/feature — отдельный branch; задача в фазе — sub-branch
  `phase-N/X.Y-description`.
- Перед созданием нового branch — метка в главном branch с префиксом `pre_`.
- `main` всегда зелёный (собирается и проходит тесты).
- Merge с `--no-ff`.
- Перед merge в `main`: обновить версию в `Cargo.toml` и добавить запись в
  `CHANGELOG.md` (на английском).
- После merge в `main`: поставить метку `vX.Y.Z`.

## Код

- Минимальные изменения, без побочных правок в несвязанном коде.
- Не добавлять зависимости без явной причины; избегать дублирования кода.
- Использовать существующие компоненты, не плодить дубликаты.
