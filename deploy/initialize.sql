\set ON_ERROR_STOP on
\set owner_password `cat /run/secrets/owner_password`
\set runtime_password `cat /run/secrets/runtime_password`
CREATE ROLE darkhorse_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD :'owner_password';
CREATE ROLE darkhorse_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION PASSWORD :'runtime_password';
CREATE DATABASE darkhorse OWNER darkhorse_owner ENCODING 'UTF8';
REVOKE ALL ON DATABASE darkhorse FROM PUBLIC;
GRANT CONNECT ON DATABASE darkhorse TO darkhorse_runtime;
\connect darkhorse
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
ALTER SCHEMA public OWNER TO darkhorse_owner;
