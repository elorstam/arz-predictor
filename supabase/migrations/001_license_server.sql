create extension if not exists pgcrypto;

create table if not exists public.licenses (
  id uuid primary key default gen_random_uuid(),
  key_hash text unique not null,
  plan text not null check (plan in ('MONTHLY', 'YEARLY', 'LIFETIME')),
  status text not null default 'UNUSED' check (status in ('UNUSED', 'ACTIVE', 'SUSPENDED', 'REVOKED', 'EXPIRED')),
  created_at timestamptz not null default now(),
  activated_at timestamptz,
  expires_at timestamptz,
  device_fingerprint_hash text,
  device_binding_version bigint not null default 0,
  last_verified_at timestamptz,
  revoked_at timestamptz,
  suspended_at timestamptz,
  metadata jsonb,
  updated_at timestamptz not null default now(),
  constraint lifetime_has_no_expiry check (plan <> 'LIFETIME' or expires_at is null)
);

create index if not exists licenses_status_idx on public.licenses(status);
create index if not exists licenses_expires_at_idx on public.licenses(expires_at);
create index if not exists licenses_device_fingerprint_idx on public.licenses(device_fingerprint_hash);

create table if not exists public.license_audit_log (
  id bigint generated always as identity primary key,
  license_id uuid references public.licenses(id) on delete set null,
  action text not null check (action in ('GENERATED', 'ACTIVATED', 'VERIFIED', 'SUSPENDED', 'REACTIVATED', 'REVOKED', 'EXTENDED', 'DEVICE_RESET', 'ACTIVATION_REJECTED')),
  created_at timestamptz not null default now(),
  actor_type text not null,
  actor_id text,
  old_status text,
  new_status text,
  device_fingerprint_hash text,
  metadata jsonb
);

create index if not exists license_audit_license_idx on public.license_audit_log(license_id, created_at desc);
create index if not exists license_audit_action_idx on public.license_audit_log(action);

alter table public.licenses enable row level security;
alter table public.license_audit_log enable row level security;

create or replace function public.v_plan_expired(p_plan text, p_expires_at timestamptz)
returns boolean language sql stable as $$
  select p_plan <> 'LIFETIME' and p_expires_at is not null and p_expires_at <= now()
$$;

create or replace function public.claim_license_activation(
  p_key_hash text,
  p_device_fingerprint_hash text
)
returns table(result_code text, license_id uuid)
language plpgsql
security definer
set search_path = public
as $$
declare
  v_license public.licenses%rowtype;
begin
  select * into v_license from public.licenses where key_hash = p_key_hash for update;
  if not found then
    return query select 'LICENSE_NOT_FOUND'::text, null::uuid;
    return;
  end if;

  if v_plan_expired(v_license.plan, v_license.expires_at) and v_license.status not in ('REVOKED', 'SUSPENDED', 'EXPIRED') then
    update public.licenses set status = 'EXPIRED', updated_at = now() where id = v_license.id;
    v_license.status := 'EXPIRED';
  end if;

  if v_license.status = 'SUSPENDED' then return query select 'LICENSE_SUSPENDED'::text, v_license.id; return; end if;
  if v_license.status = 'REVOKED' then return query select 'LICENSE_REVOKED'::text, v_license.id; return; end if;
  if v_license.status = 'EXPIRED' then return query select 'LICENSE_EXPIRED'::text, v_license.id; return; end if;

  if v_license.status = 'UNUSED' then
    update public.licenses
      set status = 'ACTIVE', activated_at = coalesce(activated_at, now()),
          device_fingerprint_hash = p_device_fingerprint_hash, last_verified_at = now(), updated_at = now()
      where id = v_license.id;
    return query select 'ACTIVE'::text, v_license.id;
    return;
  end if;

  if v_license.device_fingerprint_hash = p_device_fingerprint_hash then
    update public.licenses set last_verified_at = now(), updated_at = now() where id = v_license.id;
    return query select 'ACTIVE'::text, v_license.id;
  end if;

  return query select 'DEVICE_ALREADY_BOUND'::text, v_license.id;
end;
$$;

revoke all on function public.claim_license_activation(text, text) from public, anon, authenticated;
revoke all on function public.v_plan_expired(text, timestamptz) from public, anon, authenticated;
grant execute on function public.claim_license_activation(text, text) to service_role;
