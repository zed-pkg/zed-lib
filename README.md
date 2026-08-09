# zed-lib — moved to `zed-lib-core`

> **Historical repository.** New development and releases have moved to
> [`zed-pkg/zed-lib-core`](https://github.com/zed-pkg/zed-lib-core). This
> repository remains available so existing commit pins, branches, issues, pull
> requests, and audit links continue to resolve.

`zed-lib-core` is the semantic merge of this repository and the core ORM lineage
formerly published from [`zed-pkg/zed-orm-core`](https://github.com/zed-pkg/zed-orm-core).
It preserves both Git histories and is the only repository-level release
authority for the shared Zed library behavior, conformance corpus, and registry
ORM.

## Consumer migration

Change only the Git source. The language package names remain compatible.

| Consumer | Previous source | Canonical source/path |
| --- | --- | --- |
| Rust behavior crate `zed-lib` | `zed-pkg/zed-lib`, `src/rust` | `zed-pkg/zed-lib-core`, `src/rust` |
| TypeScript `@zed-pkg/zed-lib` | `zed-pkg/zed-lib`, `src/ts` | `zed-pkg/zed-lib-core`, `src/ts` |
| Dart `zed_lib` | `zed-pkg/zed-lib`, `src/dart` | `zed-pkg/zed-lib-core`, `src/dart` |
| Language-neutral corpus | `zed-pkg/zed-lib`, `conformance` | `zed-pkg/zed-lib-core`, `conformance` |

Example Rust Git dependency after migration:

```toml
zed-lib = {
  git = "https://github.com/zed-pkg/zed-lib-core.git",
  rev = "<reviewed-zed-lib-core-commit>"
}
```

The repository metadata inside the Rust, TypeScript, and Dart slices now points
to `zed-lib-core`.

## Preserved merge history

The two-parent history merge is:

```text
f27f72cc65640407409d38953c8d30ee4c95f3a6
```

Parents:

```text
430aafe24b6c3ab1263f1351ab4941545f592f19  zed-lib lineage
a5dabf3685db94ffdf5ae30cb3b3e4cc1cce298f  zed-orm-core lineage
```

The conceptual fold is:

```text
9fdc5fed96b707b99b3b02e6541060831c3d70fd
```

Canonical certification merged through
[`zed-lib-core#1`](https://github.com/zed-pkg/zed-lib-core/pull/1) as:

```text
171ee6a3ba82a492409ef86e27af793574942447
```

## Open predecessor work was not discarded

### One-time invitation acceptance

Predecessor [`zed-lib#7`](https://github.com/zed-pkg/zed-lib/pull/7) was ported
into [`zed-lib-core#2`](https://github.com/zed-pkg/zed-lib-core/pull/2), merged
as:

```text
79c30f65c676f6eb304effe2a7abf969f22f2da8
```

The canonical implementation retains one-time SHA-256 tokens, verified-email
matching, generic non-enumerating failures, organization/project targets,
atomic membership creation, concurrent replay protection, and no role
downgrade. It additionally uses the canonical opaque ORM context, `zed_*` schema,
revocation checks, and accepted-by evidence.

### Registry data plane

The substantive requirements from predecessor
[`zed-lib#5`](https://github.com/zed-pkg/zed-lib/pull/5) are mapped and adapted
in [`zed-lib-core#3`](https://github.com/zed-pkg/zed-lib-core/pull/3).

The canonical port uses the shared `zed_*` schema and opaque read/write contexts.
It retains upload/download/license/embedding operations and visibility-aware
text/semantic search, while retiring the transitional branch's unprefixed
schema, branch-owned migrations, raw SeaORM sessions, duplicate identity model,
and unavailable pgvector assumptions.

The complete item-by-item mapping is in
[`PREDECESSOR_MIGRATION.md`](https://github.com/zed-pkg/zed-lib-core/blob/main/PREDECESSOR_MIGRATION.md).

## Repository policy

- Do not open new feature or release work here.
- Do not publish a new package or repository-level release from this repository.
- Open historical links remain available for audit and migration.
- New bugs, features, and pull requests belong in
  [`zed-pkg/zed-lib-core`](https://github.com/zed-pkg/zed-lib-core).
- This repository may be archived only after the canonical registry-data-plane
  port is merged and every unique predecessor issue or pull request has a
  recorded canonical disposition.

## License

MIT
