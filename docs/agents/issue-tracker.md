# Issue Tracker: GitHub

Issues and PRDs for this repo live in GitHub Issues for `juanakajuan/ai-usage`.

Use the `gh` CLI to read and update issues. The repo has `origin` configured as:

```text
git@github.com:juanakajuan/ai-usage.git
```

Prefer discovering the repository from the local remote:

```bash
gh issue list --repo juanakajuan/ai-usage
```

If remote discovery is unavailable, pass the repository explicitly:

```bash
gh issue list --repo juanakajuan/ai-usage
```

## Conventions

- Use GitHub issue titles for issue summaries.
- Use GitHub issue bodies for PRDs, acceptance criteria, implementation notes, and reproduction details.
- Use GitHub labels for triage state. See `triage-labels.md` for the role strings.
- Add comments in GitHub for follow-up discussion instead of creating local tracker files.

## When a skill says "publish to the issue tracker"

Create or update a GitHub issue with `gh`.

For a new issue:

```bash
gh issue create --repo juanakajuan/ai-usage --title "<title>" --body "<body>"
```

Apply labels at creation time when the triage state is known:

```bash
gh issue create --repo juanakajuan/ai-usage --title "<title>" --body "<body>" --label "<label>"
```

## When a skill says "fetch the relevant ticket"

Read the GitHub issue with `gh`:

```bash
gh issue view <issue-number> --repo juanakajuan/ai-usage --comments
```

Use JSON output when a skill needs structured issue data:

```bash
gh issue view <issue-number> --repo juanakajuan/ai-usage --comments --json number,title,body,labels,state,comments
```

## When a skill says "apply a triage label"

Use `gh issue edit` with the mapped label from `triage-labels.md`:

```bash
gh issue edit <issue-number> --repo juanakajuan/ai-usage --add-label "<label>"
```

Remove obsolete triage labels when moving an issue to a different triage state:

```bash
gh issue edit <issue-number> --repo juanakajuan/ai-usage --remove-label "<old-label>" --add-label "<new-label>"
```
