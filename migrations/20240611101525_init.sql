CREATE TABLE categories (
	id SERIAL PRIMARY KEY,
	name TEXT NOT NULL,
	emoji TEXT NOT NULL,
	created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT now()
);

CREATE TABLE ozbargain_posts(
	id SERIAL PRIMARY KEY,
	created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT now(),
	title TEXT NOT NULL,
	description TEXT NOT NULL,
	thumbnail TEXT,
	link TEXT NOT NULL,
	ozbargain_id TEXT NOT NULL UNIQUE,
	publication_date TIMESTAMP WITHOUT TIME ZONE NOT NULL,
	notified BOOLEAN NOT NULL
);

CREATE TABLE category_association (
	id SERIAL PRIMARY KEY,
	category_id INTEGER REFERENCES categories(id),
	post_id INTEGER REFERENCES ozbargain_posts(id),
	created_at TIMESTAMP WITHOUT TIME ZONE DEFAULT now()
);

INSERT INTO categories (name, emoji) VALUES ('All', '🌏');
INSERT INTO categories (name, emoji) VALUES ('Alcohol','🍺');
INSERT INTO categories (name, emoji) VALUES ('Automotive', '🏎️');
INSERT INTO categories (name, emoji) VALUES ('Books & Magazines', '📚');
INSERT INTO categories (name, emoji) VALUES ('Computing', '🖥️');
INSERT INTO categories (name, emoji) VALUES ('Dining & Takeaway', '🥡');
INSERT INTO categories (name, emoji) VALUES ('Education', '🧮');
INSERT INTO categories (name, emoji) VALUES ('Electrical & Electronics', '🔌');
INSERT INTO categories (name, emoji) VALUES ('Entertainment', '💃');
INSERT INTO categories (name, emoji) VALUES ('Fashion & Apparel', '👜');
INSERT INTO categories (name, emoji) VALUES ('Financial', '💸');
INSERT INTO categories (name, emoji) VALUES ('Gaming', '🎮');
INSERT INTO categories (name, emoji) VALUES ('Groceries', '🛍️');
INSERT INTO categories (name, emoji) VALUES ('Health & Beauty', '🏥');
INSERT INTO categories (name, emoji) VALUES ('Home & Garden', '🏡');
INSERT INTO categories (name, emoji) VALUES ('Internet', '🌐');
INSERT INTO categories (name, emoji) VALUES ('Mobile', '📱');
INSERT INTO categories (name, emoji) VALUES ('Pets', '🐈');
INSERT INTO categories (name, emoji) VALUES ('Sports & Outdoors', '🏏');
INSERT INTO categories (name, emoji) VALUES ('Toys & Kids', '🪅');
INSERT INTO categories (name, emoji) VALUES ('Travel', '🛫');
INSERT INTO categories (name, emoji) VALUES ('Other', '🎲');

