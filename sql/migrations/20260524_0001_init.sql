create table users(
    id integer primary key autoincrement,
    thirdparty_id text not null unique,

    token_id integer not null
) strict;

insert into users
(id, thirdparty_id, token_id) values
(0,  "root",        0);

-- TODO: just use bot token to indicate manager?
create table user_is_managers(
    user_id integer primary key
        references users(id) on delete restrict
) strict;

insert into user_is_managers
(user_id) values
(0);

create table submits(
    id integer primary key autoincrement,
    user_id integer not null
        references users(id) on delete restrict,

    nth integer not null,
    user_signature text not null,
    harmony_group_intention integer not null,

    -- BLAKE3
    file_hash blob not null,
    file_mime_type text not null,
    -- RFC 3339
    created_at text not null,
    unique(user_id, nth)
) strict;

create table submit_rejects(
    submit_id integer primary key
        references submits(id) on delete restrict
) strict;

create table submit_passes(
    id integer primary key autoincrement,
    submit_id integer not null unique
        references submits(id) on delete restrict
) strict;

create table submit_pass_groups(
    id integer primary key autoincrement,
    pass_id integer not null
        references submit_passes(id) on delete restrict,
    group_name text not null,
    unique(pass_id, group_name)
) strict;

create table submit_replaces(
    submit_id integer primary key
        references submits(id) on delete restrict
) strict;

create view submits_info_with_marking
as select
  s.id,
  s.user_id,
  s.nth,
  s.user_signature,
  cast(s.harmony_group_intention as boolean) as harmony_group_intention,
  s.created_at,
  cast(
    exists (
      select 1
      from submit_rejects sr
      where sr.submit_id = s.id
    ) as boolean
  ) as rejected,
  cast(
    exists (
      select 1
      from submit_passes sp
      where sp.submit_id = s.id
    ) as boolean
  ) as passed,
  cast(
    exists (
      select 1
      from submit_replaces srep
      where srep.submit_id = s.id
    ) as boolean
  ) as replaced
from submits s;
