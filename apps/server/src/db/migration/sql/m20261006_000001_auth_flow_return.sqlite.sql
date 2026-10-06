-- Where to send the browser once a sign-in at a provider succeeds.
--
-- A page that asks somebody to sign in again, to confirm a change to their
-- account, wants them back on that page afterwards rather than on the home
-- page. Only a path on this site is stored; see `api::oidc::site_path`.
ALTER TABLE auth_flows ADD COLUMN return_to TEXT;
