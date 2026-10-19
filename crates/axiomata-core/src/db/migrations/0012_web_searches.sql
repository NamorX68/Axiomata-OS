-- Every web search Glut's `web_search` ran (docs/plans/glut-round.md, W3/W7): the app counts the searches of the local
-- day against `agents.web.daily_search_limit`, and Tavily's of the local month against `tavily_monthly_limit`, so no
-- search costs money the owner did not mean to spend.
CREATE TABLE web_searches (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 'exa' | 'tinyfish' | 'tavily'
    provider    TEXT    NOT NULL,
    -- RFC 3339, UTC.
    searched_at TEXT    NOT NULL
);
CREATE INDEX idx_web_searches_searched_at ON web_searches (searched_at);
