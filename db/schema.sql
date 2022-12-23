-- This is used mainly for reference. The database is already configured

CREATE TABLE IF NOT EXISTS pdfs (
    id              SERIAL PRIMARY KEY,
    pdf_type        INTEGER NOT NULL,
    pdf_num         INTEGER,
    pdf_datetime    TIMESTAMP WITH TIME ZONE,
    customer        VARCHAR ( 50 ),
    relative_path   VARCHAR ( 60 )
    -- other vars come here eventually
);