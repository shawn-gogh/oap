---
name: coding-agent
description: Standard procedures for a coding agent — memory hygiene, PR workflow, and automatic live preview after every change.
---

## Memory hygiene

Before starting any task, search for relevant prior context:
```
search_memory("relevant keywords for the task")
```

After completing significant work, save non-obvious findings:
```
save_memory({ content: "...", tags: ["relevant-area"] })
```

## PR workflow

1. Create a branch: `git checkout -b <descriptive-name>`
2. Make changes, commit: `git add -A && git commit -m "..."`
3. Push: `git push -u origin HEAD`
4. Open PR: `gh pr create --title "..." --body "..."`
5. **Immediately after opening the PR**, run the expose-preview procedure below — do not wait to be asked.

## Expose preview (auto, after every PR)

After opening a PR, do this automatically:

1. Identify the repo's run command and port. Check in order:
   - `package.json` scripts (`dev`, `start`, `preview`) → typically port 3000
   - `Makefile` targets (`serve`, `run`, `dev`)
   - `README.md` "getting started" / "running locally" section
   - Python/uv projects: look for `uvicorn`, `flask run`, `fastapi`, `litellm` invocations
   - If the repo has a `.env.example` or `docker-compose.yml`, check for port mappings
2. Start the service in the background:
   ```bash
   nohup <run command> > /tmp/preview.log 2>&1 &
   ```
3. Wait up to 60s for it to accept connections:
   ```bash
   PORT=<detected_port>
   for i in $(seq 1 30); do curl -sf http://localhost:$PORT/ && break || sleep 2; done
   ```
4. Register the preview so the UI shows a "View Preview" link:
   ```
   report_preview_url({ port: <detected_port> })
   ```
5. Tell the user the service is live and the preview button is active in the session header.

If you cannot determine the run command, ask the user before proceeding.
