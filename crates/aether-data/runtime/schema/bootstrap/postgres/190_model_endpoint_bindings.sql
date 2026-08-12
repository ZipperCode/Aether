CREATE TABLE IF NOT EXISTS public.model_endpoint_bindings (
    model_id character varying(64) NOT NULL,
    endpoint_id character varying(64) NOT NULL,
    source character varying(32) NOT NULL DEFAULT 'migration',
    is_active boolean NOT NULL DEFAULT TRUE,
    created_at timestamp with time zone NOT NULL DEFAULT NOW(),
    updated_at timestamp with time zone NOT NULL DEFAULT NOW(),
    CONSTRAINT model_endpoint_bindings_pkey PRIMARY KEY (model_id, endpoint_id),
    CONSTRAINT model_endpoint_bindings_model_id_fkey
        FOREIGN KEY (model_id) REFERENCES public.models(id) ON DELETE CASCADE,
    CONSTRAINT model_endpoint_bindings_endpoint_id_fkey
        FOREIGN KEY (endpoint_id) REFERENCES public.provider_endpoints(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS model_endpoint_bindings_endpoint_active_idx
    ON public.model_endpoint_bindings (endpoint_id, is_active, model_id);
