\set ON_ERROR_STOP on
\set owner_password `cat /run/secrets/owner_password`
\set operator_password `cat /run/secrets/operator_password`
\set runtime_password `cat /run/secrets/runtime_password`
CREATE ROLE darkhorse_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD :'owner_password';
CREATE ROLE darkhorse_runtime LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD :'runtime_password';
CREATE ROLE darkhorse_operator LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD :'operator_password';
CREATE DATABASE darkhorse OWNER darkhorse_owner ENCODING 'UTF8';
REVOKE ALL ON DATABASE darkhorse FROM PUBLIC;
GRANT CONNECT ON DATABASE darkhorse TO darkhorse_runtime,darkhorse_operator;
\connect darkhorse
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
ALTER SCHEMA public OWNER TO darkhorse_owner;
