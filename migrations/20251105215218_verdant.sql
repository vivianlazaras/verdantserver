ALTER TABLE "login_session" ADD COLUMN "transcript" character varying;
UPDATE "login_session" SET "transcript" = /* TODO set a value before setting the column to null */ WHERE true;
ALTER TABLE "login_session" ALTER COLUMN "transcript" SET NOT NULL;