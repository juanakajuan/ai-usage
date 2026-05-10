# Issue Tracker: Codeberg

Issues and PRDs for this repo live in Codeberg Issues for `juanakajuan/ai-usage`.

Use the `tea` CLI to read and update issues. The repo has `origin` configured as:

```text
ssh://git@codeberg.org/juanakajuan/ai-usage.git
```

Prefer discovering the repository from the local remote:

```bash
tea issues --remote origin
```

If remote discovery is unavailable, pass the repository explicitly:

```bash
tea issues --repo juanakajuan/ai-usage
```

## Conventions

- Use Codeberg issue titles for issue summaries.
- Use Codeberg issue bodies for PRDs, acceptance criteria, implementation notes, and reproduction details.
- Use Codeberg labels for triage state. See `triage-labels.md` for the role strings.
- Add comments in Codeberg for follow-up discussion instead of creating local tracker files.

## When a skill says "publish to the issue tracker"

Create or update a Codeberg issue with `tea`.

For a new issue:

```bash
tea issues create --remote origin --title "<title>" --description "<body>"
```

Apply labels at creation time when the triage state is known:

```bash
tea issues create --remote origin --title "<title>" --description "<body>" --labels "<label>"
```

## When a skill says "fetch the relevant ticket"

Read the Codeberg issue with `tea`:

```bash
tea issues --remote origin <issue-number> --comments
```

Use JSON output when a skill needs structured issue data:

```bash
tea issues --remote origin <issue-number> --comments --output json
```

## When a skill says "apply a triage label"

Use `tea issues edit` with the mapped label from `triage-labels.md`:

```bash
tea issues edit --remote origin <issue-number> --add-labels "<label>"
```

Remove obsolete triage labels when moving an issue to a different triage state:

```bash
tea issues edit --remote origin <issue-number> --remove-labels "<old-label>" --add-labels "<new-label>"
```
