-- This is used mainly for reference. The database is already configured

CREATE TABLE IF NOT EXISTS pdfs (
    id              SERIAL PRIMARY KEY,
    pdf_type        INT4,
    pdf_num         INT4,
    pdf_datetime    TIMESTAMP WITH TIME ZONE,
    customer        VARCHAR ( 150 ),
    relative_path   VARCHAR ( 200 ),
    crc32_checksum  INT4
    -- other vars come here eventually
);

CREATE TABLE IF NOT EXISTS users (
    username            VARCHAR(30) PRIMARY KEY,
    password_hash       CHAR(106),
    reset_on_next_login BOOLEAN
);

CREATE TABLE IF NOT EXISTS cookies (
    cookie              CHAR(300) PRIMARY KEY,
    username            VARCHAR(30),
    authenticated       BOOLEAN,
    created             TIMESTAMP
);