# Sample output

Run the renderer against the current repo to regenerate local sample pages:

```bash
examples/contract-consumers/html/render.sh
```

The generated HTML lives under `.bv/audience-html/`, which is gitignored. The
committed golden fixtures under `tests/expected/` cover the stable rendered
shape without committing current-repo data snapshots.
