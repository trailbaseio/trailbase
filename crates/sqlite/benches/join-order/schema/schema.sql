CREATE TABLE aka_name (
    id integer NOT NULL PRIMARY KEY,
    person_id integer NOT NULL,
    name text NOT NULL,
    imdb_index text,
    name_pcode_cf text,
    name_pcode_nf text,
    surname_pcode text,
    md5sum text
) STRICT;

CREATE TABLE aka_title (
    id integer NOT NULL PRIMARY KEY,
    movie_id integer NOT NULL,
    title text NOT NULL,
    imdb_index text,
    kind_id integer NOT NULL,
    production_year integer,
    phonetic_code text,
    episode_of_id integer,
    season_nr integer,
    episode_nr integer,
    note text,
    md5sum text
) STRICT;

CREATE TABLE cast_info (
    id integer NOT NULL PRIMARY KEY,
    person_id integer NOT NULL,
    movie_id integer NOT NULL,
    person_role_id integer,
    note text,
    nr_order integer,
    role_id integer NOT NULL
) STRICT;

CREATE TABLE char_name (
    id integer NOT NULL PRIMARY KEY,
    name text NOT NULL,
    imdb_index text,
    imdb_id integer,
    name_pcode_nf text,
    surname_pcode text,
    md5sum text
) STRICT;

CREATE TABLE comp_cast_type (
    id integer NOT NULL PRIMARY KEY,
    kind text NOT NULL
) STRICT;

CREATE TABLE company_name (
    id integer NOT NULL PRIMARY KEY,
    name text NOT NULL,
    country_code text,
    imdb_id integer,
    name_pcode_nf text,
    name_pcode_sf text,
    md5sum text
) STRICT;

CREATE TABLE company_type (
    id integer NOT NULL PRIMARY KEY,
    kind text NOT NULL
) STRICT;

CREATE TABLE complete_cast (
    id integer NOT NULL PRIMARY KEY,
    movie_id integer,
    subject_id integer NOT NULL,
    status_id integer NOT NULL
) STRICT;

CREATE TABLE info_type (
    id integer NOT NULL PRIMARY KEY,
    info text NOT NULL
) STRICT;

CREATE TABLE keyword (
    id integer NOT NULL PRIMARY KEY,
    keyword text NOT NULL,
    phonetic_code text
) STRICT;

CREATE TABLE kind_type (
    id integer NOT NULL PRIMARY KEY,
    kind text NOT NULL
) STRICT;

CREATE TABLE link_type (
    id integer NOT NULL PRIMARY KEY,
    link text NOT NULL
) STRICT;

CREATE TABLE movie_companies (
    id integer NOT NULL PRIMARY KEY,
    movie_id integer NOT NULL,
    company_id integer NOT NULL,
    company_type_id integer NOT NULL,
    note text
) STRICT;

CREATE TABLE movie_info (
    id integer NOT NULL PRIMARY KEY,
    movie_id integer NOT NULL,
    info_type_id integer NOT NULL,
    info text NOT NULL,
    note text
);

CREATE TABLE movie_info_idx (
    id integer NOT NULL PRIMARY KEY,
    movie_id integer NOT NULL,
    info_type_id integer NOT NULL,
    info text NOT NULL,
    note text
) STRICT;

CREATE TABLE movie_keyword (
    id integer NOT NULL PRIMARY KEY,
    movie_id integer NOT NULL,
    keyword_id integer NOT NULL
) STRICT;

CREATE TABLE movie_link (
    id integer NOT NULL PRIMARY KEY,
    movie_id integer NOT NULL,
    linked_movie_id integer NOT NULL,
    link_type_id integer NOT NULL
) STRICT;

CREATE TABLE name (
    id integer NOT NULL PRIMARY KEY,
    name text NOT NULL,
    imdb_index text,
    imdb_id integer,
    gender text,
    name_pcode_cf text,
    name_pcode_nf text,
    surname_pcode text,
    md5sum text
) STRICT;

CREATE TABLE person_info (
    id integer NOT NULL PRIMARY KEY,
    person_id integer NOT NULL,
    info_type_id integer NOT NULL,
    info text NOT NULL,
    note text
) STRICT;

CREATE TABLE role_type (
    id integer NOT NULL PRIMARY KEY,
    role text NOT NULL
) STRICT;

CREATE TABLE title (
    id integer NOT NULL PRIMARY KEY,
    title text NOT NULL,
    imdb_index text,
    kind_id integer NOT NULL,
    production_year integer,
    imdb_id integer,
    phonetic_code text,
    episode_of_id integer,
    season_nr integer,
    episode_nr integer,
    series_years text,
    md5sum text
) STRICT;
