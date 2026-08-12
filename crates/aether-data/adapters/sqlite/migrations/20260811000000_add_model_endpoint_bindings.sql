CREATE TABLE IF NOT EXISTS model_endpoint_bindings (
    model_id TEXT NOT NULL,
    endpoint_id TEXT NOT NULL,
    source TEXT NOT NULL DEFAULT 'migration',
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (model_id, endpoint_id),
    FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE,
    FOREIGN KEY (endpoint_id) REFERENCES provider_endpoints(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS model_endpoint_bindings_endpoint_active_idx
    ON model_endpoint_bindings (endpoint_id, is_active, model_id);

INSERT OR IGNORE INTO model_endpoint_bindings (
    model_id,
    endpoint_id,
    source,
    is_active,
    created_at,
    updated_at
)
SELECT DISTINCT
    model.id,
    endpoint.id,
    'mapping',
    1,
    CAST(strftime('%s', 'now') AS INTEGER),
    CAST(strftime('%s', 'now') AS INTEGER)
FROM models AS model
INNER JOIN json_each(
    CASE
        WHEN json_valid(model.provider_model_mappings)
         AND json_type(model.provider_model_mappings) = 'array'
            THEN model.provider_model_mappings
        ELSE '[]'
    END
) AS mapping
INNER JOIN json_each(
    CASE
        WHEN mapping.type = 'object' AND json_type(mapping.value, '$.endpoint_ids') = 'array'
            THEN json_extract(mapping.value, '$.endpoint_ids')
        ELSE '[]'
    END
) AS mapped_endpoint
INNER JOIN provider_endpoints AS endpoint
    ON endpoint.id = TRIM(CAST(mapped_endpoint.value AS TEXT))
   AND endpoint.provider_id = model.provider_id
WHERE NULLIF(TRIM(CAST(mapped_endpoint.value AS TEXT)), '') IS NOT NULL;

INSERT OR IGNORE INTO model_endpoint_bindings (
    model_id,
    endpoint_id,
    source,
    is_active,
    created_at,
    updated_at
)
SELECT DISTINCT
    model.id,
    endpoint.id,
    'mapping',
    1,
    CAST(strftime('%s', 'now') AS INTEGER),
    CAST(strftime('%s', 'now') AS INTEGER)
FROM models AS model
INNER JOIN json_each(
    CASE
        WHEN json_valid(model.provider_model_mappings)
         AND json_type(model.provider_model_mappings) = 'array'
            THEN model.provider_model_mappings
        ELSE '[]'
    END
) AS mapping
INNER JOIN json_each(
    CASE
        WHEN mapping.type = 'object' AND json_type(mapping.value, '$.api_formats') = 'array'
            THEN json_extract(mapping.value, '$.api_formats')
        ELSE '[]'
    END
) AS mapped_format
INNER JOIN provider_endpoints AS endpoint
    ON endpoint.provider_id = model.provider_id
   AND LOWER(TRIM(COALESCE(endpoint.api_format, '')))
       = LOWER(TRIM(CAST(mapped_format.value AS TEXT)))
WHERE NULLIF(TRIM(CAST(mapped_format.value AS TEXT)), '') IS NOT NULL
  AND NOT EXISTS (
      SELECT 1
      FROM json_each(
          CASE
              WHEN json_valid(model.provider_model_mappings)
               AND json_type(model.provider_model_mappings) = 'array'
                  THEN model.provider_model_mappings
              ELSE '[]'
          END
      ) AS mapping
      INNER JOIN json_each(
          CASE
              WHEN mapping.type = 'object' AND json_type(mapping.value, '$.endpoint_ids') = 'array'
                  THEN json_extract(mapping.value, '$.endpoint_ids')
              ELSE '[]'
          END
      ) AS mapped_endpoint
      WHERE NULLIF(TRIM(CAST(mapped_endpoint.value AS TEXT)), '') IS NOT NULL
  );

INSERT OR IGNORE INTO model_endpoint_bindings (
    model_id,
    endpoint_id,
    source,
    is_active,
    created_at,
    updated_at
)
SELECT
    model.id,
    endpoint.id,
    'migration',
    1,
    CAST(strftime('%s', 'now') AS INTEGER),
    CAST(strftime('%s', 'now') AS INTEGER)
FROM models AS model
INNER JOIN provider_endpoints AS endpoint
    ON endpoint.provider_id = model.provider_id
   AND LOWER(TRIM(COALESCE(endpoint.api_format, '')))
       = LOWER(TRIM(COALESCE(model.api_format, '')))
WHERE NULLIF(TRIM(model.api_format), '') IS NOT NULL
  AND NOT EXISTS (
    SELECT 1
    FROM json_each(
        CASE
            WHEN json_valid(model.provider_model_mappings)
             AND json_type(model.provider_model_mappings) = 'array'
                THEN model.provider_model_mappings
            ELSE '[]'
        END
    ) AS mapping
    INNER JOIN json_each(
        CASE
            WHEN mapping.type = 'object' AND json_type(mapping.value, '$.endpoint_ids') = 'array'
                THEN json_extract(mapping.value, '$.endpoint_ids')
            ELSE '[]'
        END
    ) AS mapped_endpoint
    WHERE NULLIF(TRIM(CAST(mapped_endpoint.value AS TEXT)), '') IS NOT NULL
)
  AND NOT EXISTS (
    SELECT 1
    FROM json_each(
        CASE
            WHEN json_valid(model.provider_model_mappings)
             AND json_type(model.provider_model_mappings) = 'array'
                THEN model.provider_model_mappings
            ELSE '[]'
        END
    ) AS mapping
    INNER JOIN json_each(
        CASE
            WHEN mapping.type = 'object' AND json_type(mapping.value, '$.api_formats') = 'array'
                THEN json_extract(mapping.value, '$.api_formats')
            ELSE '[]'
        END
    ) AS mapped_format
    WHERE NULLIF(TRIM(CAST(mapped_format.value AS TEXT)), '') IS NOT NULL
  );

INSERT OR IGNORE INTO model_endpoint_bindings (
    model_id,
    endpoint_id,
    source,
    is_active,
    created_at,
    updated_at
)
SELECT
    model.id,
    endpoint.id,
    'migration',
    1,
    CAST(strftime('%s', 'now') AS INTEGER),
    CAST(strftime('%s', 'now') AS INTEGER)
FROM models AS model
INNER JOIN provider_endpoints AS endpoint
    ON endpoint.provider_id = model.provider_id
WHERE NOT EXISTS (
    SELECT 1
    FROM json_each(
        CASE
            WHEN json_valid(model.provider_model_mappings)
             AND json_type(model.provider_model_mappings) = 'array'
                THEN model.provider_model_mappings
            ELSE '[]'
        END
    ) AS mapping
    INNER JOIN json_each(
        CASE
            WHEN mapping.type = 'object' AND json_type(mapping.value, '$.endpoint_ids') = 'array'
                THEN json_extract(mapping.value, '$.endpoint_ids')
            ELSE '[]'
        END
    ) AS mapped_endpoint
    WHERE NULLIF(TRIM(CAST(mapped_endpoint.value AS TEXT)), '') IS NOT NULL
)
  AND NOT EXISTS (
    SELECT 1
    FROM json_each(
        CASE
            WHEN json_valid(model.provider_model_mappings)
             AND json_type(model.provider_model_mappings) = 'array'
                THEN model.provider_model_mappings
            ELSE '[]'
        END
    ) AS mapping
    INNER JOIN json_each(
        CASE
            WHEN mapping.type = 'object' AND json_type(mapping.value, '$.api_formats') = 'array'
                THEN json_extract(mapping.value, '$.api_formats')
            ELSE '[]'
        END
    ) AS mapped_format
    WHERE NULLIF(TRIM(CAST(mapped_format.value AS TEXT)), '') IS NOT NULL
  )
  AND NULLIF(TRIM(model.api_format), '') IS NULL;
