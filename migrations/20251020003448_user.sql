CREATE TABLE "user" (
"id" uuid NOT NULL PRIMARY KEY,
"username" character varying NOT NULL,
"email" character varying NOT NULL,
"subject" uuid NOT NULL,
"password_hash" character varying NOT NULL
);
CREATE TABLE "room" (
"id" uuid NOT NULL PRIMARY KEY,
"name" character varying NOT NULL,
"created_at" bigint NOT NULL
);
CREATE TABLE "permission" (
"id" uuid NOT NULL PRIMARY KEY,
"user_id" uuid NOT NULL REFERENCES "user"("id"),
"room_id" uuid NOT NULL REFERENCES "room"("id"),
"room_admin" boolean NOT NULL,
"can_publish" boolean NOT NULL,
"can_subcribe" boolean NOT NULL
);