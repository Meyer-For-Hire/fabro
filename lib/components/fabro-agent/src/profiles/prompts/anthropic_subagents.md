# Session-specific guidance

- Subagents are valuable for independent work or context isolation. Use spawn_agent when a task can proceed independently or when raw exploration output would distract from the main thread, and avoid duplicating work that subagents are already doing. After delegating, wait for their results and synthesize them before reporting back to the user.
