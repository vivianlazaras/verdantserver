CREATE TABLE "auth_record" (
"id" uuid NOT NULL PRIMARY KEY,
"user_id" uuid NOT NULL REFERENCES "user"("id"),
"password_hash" character varying,
"expiration" bigint NOT NULL,
"registration" character varying
);
CREATE TABLE "login_session" (
"id" uuid NOT NULL PRIMARY KEY,
"user_id" uuid NOT NULL REFERENCES "user"("id"),
"server_login" character varying NOT NULL,
"login_start" bigint,
"session_start" bigint NOT NULL,
"session_end" bigint NOT NULL
);