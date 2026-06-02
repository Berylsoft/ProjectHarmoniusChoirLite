-- name: get_user_token_id_by_user_id :one
select token_id
from users
where id = ?;

-- name: get_user_id_by_thirdparty_id :one
select id
from users
where thirdparty_id = ?;

-- name: ins_user :one
insert into users
(thirdparty_id, token_id) values
(?,             0)
returning id, token_id;

-- name: inc_user_token_id_by_user_id :one
update users
set token_id = token_id + 1
where id = ?
returning token_id;

-- name: is_manager_by_user_id :one
select exists (
  select 1
  from user_is_managers
  where user_id = ?
) as is_manager;

-- name: get_pending_submit_by_user_id :one
select s.id, s.nth
from submits s
where s.user_id = ? and
  not exists (
    select 1
    from submit_rejects sr
    where sr.submit_id = s.id
  ) and
  not exists (
    select 1
    from submit_passes sp
    where sp.submit_id = s.id
  ) and
  not exists (
    select 1
    from submit_replaces srep
    where srep.submit_id = s.id
  );

-- name: get_max_submit_nth_by_user_id :one
select cast(coalesce(max(nth), 0) as integer) as "nth"
from submits
where user_id = ?;

-- name: ins_submit_replace :exec
insert into submit_replaces
(submit_id) values
(?);

-- name: ins_submit_reject :exec
insert into submit_rejects
(submit_id) values
(?);

-- name: ins_submit_pass :one
insert into submit_passes
(submit_id) values
(?)
returning id;

-- name: ins_submit_pass_group :exec
insert into submit_pass_groups
(pass_id, group_name) values
(?,       ?);

-- name: ins_submit :one
insert into submits
(user_id,    nth,            user_signature, harmony_group_intention,
  file_hash, file_mime_type, created_at) values
(?,          ?,              ?,              ?,
  ?,         ?,              ?)
returning id;

-- name: get_submit_for_review_by_submit_id :one
select
  user_id,
  cast(
    (not (rejected or passed or replaced)) as boolean
  ) as pending
from submits_info_with_marking
where id = ?;

-- name: get_last_passed_submit_id_by_user_id :one
-- last passed and not replaced
select s.id
from submits s
where s.user_id = ? and
  exists (
    select 1
    from submit_passes sp
    where sp.submit_id = s.id
  ) and
  not exists (
    select 1
    from submit_replaces srep
    where srep.submit_id = s.id
  );

-- name: get_passed_groups_by_submit_id :many
select spg.group_name
from submit_passes sp
join submit_pass_groups spg on spg.pass_id = sp.id
where sp.submit_id = ?;

-- name: get_all_submits :many
select *
from submits_info_with_marking 
order by id desc;

-- name: get_submit_by_id :one
select *
from submits_info_with_marking 
where id = ?;

-- name: get_previous_submit_signature_by_user_id :one
select user_signature
from submits
where user_id = ?
order by id desc
limit 1;

-- name: get_all_submits_by_user_id :many
select
  s.id,
  s.nth,
  s.user_signature,
  s.harmony_group_intention,
  s.created_at,
  s.rejected,
  s.passed,
  s.replaced
from submits_info_with_marking s
where user_id = ?
order by id desc;

-- name: get_file_info_by_submit_id :one
select *
from submits_file_info_with_marking s
where s.id = ?;

-- name: get_file_infos_of_all_passed_submits :many
select *
from submits_file_info_with_marking s
where s.passed and not s.replaced;
