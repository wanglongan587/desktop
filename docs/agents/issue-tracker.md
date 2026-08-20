# Issue tracker: GitHub

Issues and specs for this repo live in GitHub Issues for `wanglongan587/desktop`. Use the `gh` CLI for all operations and explicitly target this repository because the local clone has multiple GitHub remotes.

## Repository

```text
wanglongan587/desktop
```

Use `--repo wanglongan587/desktop` or set `GH_REPO=wanglongan587/desktop`.

## Conventions

- Create an issue with `gh issue create --repo wanglongan587/desktop`.
- Read an issue with `gh issue view <number> --repo wanglongan587/desktop --comments`.
- List issues with `gh issue list --repo wanglongan587/desktop`.
- Comment with `gh issue comment <number> --repo wanglongan587/desktop`.
- Apply or remove labels with `gh issue edit <number> --repo wanglongan587/desktop`.
- Close with `gh issue close <number> --repo wanglongan587/desktop`.

Use a file or safe multi-line input mechanism for long issue bodies. Do not rely on the clone's default remote when publishing.

## Pull requests as a triage surface

**PRs as a request surface: no.**

External pull requests are not included in the issue triage queue unless this flag is changed manually.

## When a skill says "publish to the issue tracker"

Create a GitHub issue in `wanglongan587/desktop`.

## When a skill says "fetch the relevant ticket"

Read the corresponding GitHub issue from `wanglongan587/desktop`, including its comments and labels.

## Wayfinding operations

- A map is a GitHub issue labelled `wayfinder:map`.
- Child tickets are linked as GitHub sub-issues where available.
- Child types use `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`, or `wayfinder:task`.
- Use GitHub native issue dependencies where available.
- Claim a ticket by assigning it to the current user.
- Resolve a ticket by posting the answer, closing it, and updating the map's decisions.
