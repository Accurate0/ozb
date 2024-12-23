CREATE TABLE audit (
	post_id INTEGER REFERENCES ozbargain_posts(id) ON DELETE SET NULL,
	keyword_id INTEGER REFERENCES registered_keywords(id) ON DELETE SET NULL,
	user_id INTEGER REFERENCES discord_users(id) ON DELETE SET NULL,
	created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT now()
);
