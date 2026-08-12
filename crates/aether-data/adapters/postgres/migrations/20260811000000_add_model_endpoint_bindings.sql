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

WITH mapping_rows AS (
    SELECT
        model.id AS model_id,
        model.provider_id,
        mapping.value
    FROM public.models AS model
    CROSS JOIN LATERAL jsonb_array_elements(
        CASE
            WHEN jsonb_typeof(model.provider_model_mappings) = 'array'
                THEN model.provider_model_mappings
            ELSE '[]'::jsonb
        END
    ) AS mapping(value)
),
explicit_mapping_endpoint_ids AS (
    SELECT DISTINCT
        mapping.model_id,
        mapped_endpoint.id AS endpoint_id
    FROM mapping_rows AS mapping
    CROSS JOIN LATERAL jsonb_array_elements_text(
        CASE
            WHEN jsonb_typeof(mapping.value -> 'endpoint_ids') = 'array'
                THEN mapping.value -> 'endpoint_ids'
            ELSE '[]'::jsonb
        END
    ) AS mapped_endpoint(id)
    WHERE BTRIM(mapped_endpoint.id) <> ''
),
explicit_mapping_models AS (
    SELECT DISTINCT model_id
    FROM explicit_mapping_endpoint_ids
),
explicit_mapping_bindings AS (
    SELECT
        mapped.model_id,
        endpoint.id AS endpoint_id
    FROM explicit_mapping_endpoint_ids AS mapped
    INNER JOIN public.models AS model
        ON model.id = mapped.model_id
    INNER JOIN public.provider_endpoints AS endpoint
        ON endpoint.id = mapped.endpoint_id
       AND endpoint.provider_id = model.provider_id
),
mapping_format_values AS (
    SELECT DISTINCT
        mapping.model_id,
        mapped_format.value AS api_format
    FROM mapping_rows AS mapping
    CROSS JOIN LATERAL jsonb_array_elements_text(
        CASE
            WHEN jsonb_typeof(mapping.value -> 'api_formats') = 'array'
                THEN mapping.value -> 'api_formats'
            ELSE '[]'::jsonb
        END
    ) AS mapped_format(value)
    WHERE BTRIM(mapped_format.value) <> ''
),
mapping_format_models AS (
    SELECT DISTINCT model_id
    FROM mapping_format_values
),
mapping_format_bindings AS (
    SELECT DISTINCT
        model.id AS model_id,
        endpoint.id AS endpoint_id
    FROM public.models AS model
    INNER JOIN mapping_format_values AS format
        ON format.model_id = model.id
    INNER JOIN public.provider_endpoints AS endpoint
        ON endpoint.provider_id = model.provider_id
       AND LOWER(BTRIM(COALESCE(endpoint.api_format, '')))
           = LOWER(BTRIM(format.api_format))
    WHERE NOT EXISTS (
          SELECT 1
          FROM explicit_mapping_models AS explicit
          WHERE explicit.model_id = model.id
      )
),
legacy_format_bindings AS (
    SELECT DISTINCT
        model.id AS model_id,
        endpoint.id AS endpoint_id
    FROM public.models AS model
    INNER JOIN public.provider_endpoints AS endpoint
        ON endpoint.provider_id = model.provider_id
       AND LOWER(BTRIM(COALESCE(endpoint.api_format, '')))
           = LOWER(BTRIM(COALESCE(model.api_format, '')))
    WHERE BTRIM(COALESCE(model.api_format, '')) <> ''
      AND NOT EXISTS (
          SELECT 1
          FROM explicit_mapping_models AS explicit
          WHERE explicit.model_id = model.id
      )
      AND NOT EXISTS (
          SELECT 1
          FROM mapping_format_models AS format
          WHERE format.model_id = model.id
      )
),
compatibility_bindings AS (
    SELECT
        model.id AS model_id,
        endpoint.id AS endpoint_id
    FROM public.models AS model
    INNER JOIN public.provider_endpoints AS endpoint
        ON endpoint.provider_id = model.provider_id
    WHERE NOT EXISTS (
        SELECT 1
          FROM explicit_mapping_models AS explicit
          WHERE explicit.model_id = model.id
      )
      AND NOT EXISTS (
          SELECT 1
          FROM mapping_format_models AS format
          WHERE format.model_id = model.id
      )
      AND BTRIM(COALESCE(model.api_format, '')) = ''
),
initial_bindings AS (
    SELECT model_id, endpoint_id, 'mapping'::text AS source
    FROM explicit_mapping_bindings
    UNION ALL
    SELECT model_id, endpoint_id, 'mapping'::text AS source
    FROM mapping_format_bindings
    UNION ALL
    SELECT model_id, endpoint_id, 'migration'::text AS source
    FROM legacy_format_bindings
    UNION ALL
    SELECT model_id, endpoint_id, 'migration'::text AS source
    FROM compatibility_bindings
)
INSERT INTO public.model_endpoint_bindings (
    model_id,
    endpoint_id,
    source,
    is_active,
    created_at,
    updated_at
)
SELECT model_id, endpoint_id, source, TRUE, NOW(), NOW()
FROM initial_bindings
ON CONFLICT (model_id, endpoint_id) DO NOTHING;
