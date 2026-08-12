---
name: upgrade-async-openai
description: Safely bring upstream 64bit/async-openai changes into spiceai/async-openai while preserving the Spice fork's API compatibility, retry/SSE transport, Azure authentication, and utoipa schemas. Use when upgrading the fork, syncing OpenAI API types, resolving upstream merge conflicts, or preparing the consuming Spice repository to use a new fork revision.
---

# Upgrade the Spice `async-openai` fork

Treat `spiceai` as the integration branch. Spice consumes its commits directly; `main` is not the correct base for an upgrade.

## Establish the divergence

Add the canonical upstream remote if needed, then fetch both tips.

```sh
git fetch origin spiceai
git fetch upstream main
git rev-list --left-right --count origin/spiceai...upstream/main
git log --left-only --oneline origin/spiceai...upstream/main
```

Create the upgrade branch from `origin/spiceai`, never `main`.

```sh
git switch -c <upgrade-branch> origin/spiceai
```

Record the left-only commits or their semantic categories in the PR description before merging anything.

## Choose the smallest safe import

For a focused OpenAI schema addition, transplant the relevant type and serde changes rather than merging all upstream commits. A broad sync can replace transport and configuration code unrelated to the requested API support.

For a true upstream release sync, merge `upstream/main` into the branch. Resolve every conflict deliberately; do not take either side wholesale.

## Preserve Spice behavior

Keep these behaviors unless an explicit replacement is reviewed and tested:

- Rate-limit retries, including `Retry-After` handling and aggregated retry logging.
- SSE handling through `spiceai/reqwest-eventsource`, including HTTP error mapping.
- Azure API-key and Entra-token authentication.
- `Config::api_key()` ownership and custom-header behavior used by Spice consumers.
- `utoipa::ToSchema` derives and related public Responses/OpenAPI schema compatibility.
- Compatibility fixes to response fields, `OutputItem`, service tiers, reasoning content, and embedding request traits.

When upstream changes `client.rs`, `config.rs`, `Cargo.toml`, or generated Responses types, compare call sites and feature flags before resolving. Preserve the Spice behavior above and add a regression test for any conflict that changes request construction, authentication, retrying, streaming, or serialized API shape.

## Validate

Run the narrowest relevant check first. For Responses type changes:

```sh
cargo test -p async-openai --features response-types --test responses_ser_de
cargo check -p async-openai --features responses
```

When changing client, config, dependency, retry, or SSE code, also run the appropriate wider crate tests and compile the affected examples/features. Keep the feature set consistent between commands.

Use `git diff --check` for changes authored in the upgrade. Do not reformat unrelated upstream content merely to fix inherited whitespace.

## Hand off to Spice

Open the fork PR against `spiceai`, not `main`. State the upstream range, preserved fork behavior, conflict resolutions, and exact validation commands.

After the fork PR merges, update the 40-character `async-openai` revision in the Spice repository, regenerate the lockfile as needed, and run the Responses endpoint tests there. Do not pin a consumer to an unmerged fork branch.
