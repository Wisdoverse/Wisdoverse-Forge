-- Billing Stripe reconciliation hardening.

ALTER TABLE subscriptions
    ADD COLUMN IF NOT EXISTS cancel_at_period_end BOOLEAN NOT NULL DEFAULT false;

CREATE UNIQUE INDEX IF NOT EXISTS idx_subscriptions_stripe_subscription_unique
    ON subscriptions(stripe_subscription_id)
    WHERE stripe_subscription_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_subscriptions_stripe_customer
    ON subscriptions(stripe_customer_id)
    WHERE stripe_customer_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_billing_plans_stripe_price
    ON billing_plans(stripe_price_id)
    WHERE stripe_price_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_invoices_stripe_invoice_unique
    ON invoices(stripe_invoice_id)
    WHERE stripe_invoice_id IS NOT NULL;
