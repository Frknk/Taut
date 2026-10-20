-- Paid plans of the prototype (docs/adr/0012): pro and business next to free. There is no
-- billing yet, so an operator assigns them (`just tenant-plan <slug> <plan>`); the quotas live
-- in `Plan` (services/api/src/tenant.rs). Expand-only: every existing row stays valid.

ALTER TABLE tenants DROP CONSTRAINT tenants_plan_check;
ALTER TABLE tenants ADD CONSTRAINT tenants_plan_check CHECK (plan IN ('free', 'pro', 'business'));
