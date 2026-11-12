-- Which of VS Code's harnesses kept a session that VS Code started: `copilot` for its Copilot
-- harness, whose sessions sit with Copilot CLI's, or `local` for its Local harness, whose sessions
-- are VS Code's own chat session files. Such a session keeps the agent `copilot-cli`, the name
-- versions before 2.1 can read, so that they price it by its charges; this column makes it Copilot
-- in VS Code's (ADR 0017).
ALTER TABLE sessions ADD COLUMN vscode_harness TEXT;

-- Whether a request covers one prompt and every model call it led to, as VS Code's chat session
-- files record usage: its output tokens and its charge, with neither its input nor its cache
-- tokens.
ALTER TABLE requests ADD COLUMN per_prompt INTEGER NOT NULL DEFAULT 0;

-- The Copilot sessions VS Code started on its Copilot harness, whose requests carry the client
-- that started them.
UPDATE sessions SET vscode_harness = 'copilot'
WHERE agent = 'copilot-cli'
  AND EXISTS (
      SELECT 1 FROM requests
      WHERE requests.session = sessions.id
        AND requests.entrypoint IN ('vscode-agent-host', 'vscode')
  );
