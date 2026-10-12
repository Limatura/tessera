-- Where to send the browser after a successful provider sign-in.
--
-- A page that asks the user to sign in again to confirm an account change
-- wants them back on that page afterwards, not on the home page. Only a path
-- on this site is stored; see `api::oidc::site_path`.
ALTER TABLE auth_flows ADD COLUMN return_to TEXT;
