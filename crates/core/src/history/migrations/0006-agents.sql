-- The agent each session ran in; every session recorded before then is Claude Code's.
ALTER TABLE sessions ADD COLUMN agent TEXT NOT NULL DEFAULT 'claude-code';

-- Who started a Copilot CLI request (`user`, `agent` or `sub-agent`), its reasoning tokens, which
-- its output tokens already count, and the charge it recorded, by kind of token, in nano AI
-- credits. The charge is NULL for a request without one, such as every Claude Code request.
ALTER TABLE requests ADD COLUMN reasoning_tokens INTEGER NOT NULL DEFAULT 0;
ALTER TABLE requests ADD COLUMN initiator TEXT;
ALTER TABLE requests ADD COLUMN charge_input_nano INTEGER;
ALTER TABLE requests ADD COLUMN charge_output_nano INTEGER;
ALTER TABLE requests ADD COLUMN charge_cache_read_nano INTEGER;
ALTER TABLE requests ADD COLUMN charge_cache_write_nano INTEGER;
-- `claude_code_version` holds the version of whichever agent made the request.

-- The names an agent records for its models, where the price table has none.
CREATE TABLE model_names (
    agent TEXT NOT NULL,
    model TEXT NOT NULL,
    name TEXT NOT NULL,
    PRIMARY KEY (agent, model)
) STRICT, WITHOUT ROWID;

-- An allowance's use and size in AI credits, where the agent records them besides the share used.
ALTER TABLE limit_snapshots ADD COLUMN used REAL;
ALTER TABLE limit_snapshots ADD COLUMN entitlement REAL;

-- A hash of a transcript's first line, which tells a transcript written again from the start
-- apart from one that grew.
ALTER TABLE transcripts ADD COLUMN first_line INTEGER;

-- The agent whose data a folder the person added holds.
ALTER TABLE data_folders ADD COLUMN agent TEXT NOT NULL DEFAULT 'claude-code';
