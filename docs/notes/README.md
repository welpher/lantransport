# Release notes

The Release workflow publishes a tag on its own. If a file named after the tag
exists here, it becomes the GitHub release body; otherwise the release falls back
to a generic "see the assets below" note.

```
docs/notes/v0.2.0.md   ->   gh release edit v0.2.0 --notes-file docs/notes/v0.2.0.md
```

Write it in the same PR that bumps the version, so the tag push needs no manual
step afterwards. Sections used so far: `Fixed`, `Added`, `Changed`, `Notes`.
