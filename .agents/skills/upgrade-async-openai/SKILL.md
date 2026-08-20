---
name: upgrade-async-openai
description: Safely bring upstream 64bit/async-openai changes into spiceai/async-openai while preserving the Spice fork's API compatibility, retry/SSE transport, Azure authentication, and utoipa schemas. Use when upgrading the fork, syncing OpenAI API types, resolving upstream merge conflicts, or preparing the consuming Spice repository to use a new fork revision.
---

# Upgrade the Spice `async-openai` fork

Use a two-stage upgrade. First make `main` contain the desired upstream changes. Then merge that updated `main` into `spiceai`, preserving the fork-specific behavior that Spice consumes.

## Stage 1: update `main` from upstream

Add the canonical upstream remote if needed, then fetch both tips.

```sh
git fetch origin main
git fetch upstream main
git rev-list --left-right --count origin/main...upstream/main
git switch -c <upstream-sync-branch> origin/main
git merge --no-ff upstream/main
```

Open the first PR against `main`. Keep it an upstream sync: do not add Spice-specific compatibility changes to this PR. Record the upstream commit range and validation performed.

## Stage 2: merge `main` into `spiceai`

Begin only after the Stage 1 PR merges.

```sh
git fetch origin main spiceai
git switch -c <spiceai-merge-branch> origin/spiceai
git merge --no-ff origin/main
git rev-list --left-right --count origin/spiceai...origin/main
git log --left-only --oneline origin/spiceai...origin/main
```

Open the second PR against `spiceai`. Record the Spice-only commits or their semantic categories before resolving conflicts. Resolve every conflict deliberately; do not take either side wholesale.

## Preserve Spice behavior

Keep these behaviors unless an explicit replacement is reviewed and tested:

- Rate-limit retries, including `Retry-After` handling and aggregated retry logging.
- SSE handling through `spiceai/reqwest-eventsource`, including HTTP error mapping.
- Azure API-key and Entra-token authentication.
- `Config::api_key()` ownership and custom-header behavior used by Spice consumers.
- `utoipa::ToSchema` derives and related public Responses/OpenAPI schema compatibility.
- Compatibility fixes to response fields, `OutputItem`, service tiers, reasoning content, and embedding request traits.

When upstream changes `client.rs`, `config.rs`, `Cargo.toml`, or generated Responses types, compare call sites and feature flags before resolving the Stage 2 merge. Preserve the Spice behavior above and add a regression test for any conflict that changes request construction, authentication, retrying, streaming, or serialized API shape.

## Validate

Run the narrowest relevant check first. For Responses type changes:

```sh
cargo test -p async-openai --features response-types --test responses_ser_de
cargo check -p async-openai --features responses
```

When changing client, config, dependency, retry, or SSE code, also run the appropriate wider crate tests and compile the affected examples/features. Keep the feature set consistent between commands.

Use `git diff --check` for changes authored in the upgrade. Do not reformat unrelated upstream content merely to fix inherited whitespace.

## Hand off to Spice

State the upstream range in the Stage 1 PR. State the preserved fork behavior, conflict resolutions, and exact validation commands in the Stage 2 PR.

After the Stage 2 PR merges, update the 40-character `async-openai` revision in the Spice repository, regenerate the lockfile as needed, and run the Responses endpoint tests there. Do not pin a consumer to an unmerged fork branch.
