CREATE TABLE discord_users (
	id SERIAL PRIMARY KEY,
	discord_id TEXT UNIQUE NOT NULL,
	created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT now()
);

CREATE TABLE discord_notifications (
	id SERIAL PRIMARY KEY,
	channel_id TEXT NOT NULL,
	created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT now()
);

CREATE TABLE registered_keywords (
	id SERIAL PRIMARY KEY,
	keyword TEXT NOT NULL,
	discord_user_id INTEGER REFERENCES discord_users(id) ON DELETE CASCADE NOT NULL,
	discord_notification_id INTEGER REFERENCES discord_notifications(id) NOT NULL,
	categories TEXT[] NOT NULL,
	created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT now()
);
