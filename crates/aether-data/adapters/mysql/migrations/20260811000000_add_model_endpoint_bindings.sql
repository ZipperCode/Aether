CREATE TABLE IF NOT EXISTS model_endpoint_bindings (
    model_id VARCHAR(64) NOT NULL,
    endpoint_id VARCHAR(64) NOT NULL,
    source VARCHAR(32) NOT NULL DEFAULT 'migration',
    is_active TINYINT(1) NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (model_id, endpoint_id),
    KEY model_endpoint_bindings_endpoint_active_idx (endpoint_id, is_active, model_id),
    CONSTRAINT model_endpoint_bindings_model_id_fkey
        FOREIGN KEY (model_id) REFERENCES models(id) ON DELETE CASCADE,
    CONSTRAINT model_endpoint_bindings_endpoint_id_fkey
        FOREIGN KEY (endpoint_id) REFERENCES provider_endpoints(id) ON DELETE CASCADE
);

INSERT IGNORE INTO model_endpoint_bindings (
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
    UNIX_TIMESTAMP(),
    UNIX_TIMESTAMP()
FROM models AS model
INNER JOIN JSON_TABLE(
    CASE
        WHEN JSON_VALID(model.provider_model_mappings)
         AND JSON_TYPE(model.provider_model_mappings) = 'ARRAY'
            THEN model.provider_model_mappings
        ELSE JSON_ARRAY()
    END,
    '$[*]' COLUMNS(endpoint_ids JSON PATH '$.endpoint_ids')
) AS mapping
INNER JOIN JSON_TABLE(
    CASE
        WHEN JSON_VALID(mapping.endpoint_ids)
         AND JSON_TYPE(mapping.endpoint_ids) = 'ARRAY'
            THEN mapping.endpoint_ids
        ELSE JSON_ARRAY()
    END,
    '$[*]' COLUMNS(endpoint_id VARCHAR(64) PATH '$')
) AS mapped_endpoint
INNER JOIN provider_endpoints AS endpoint
    ON BINARY endpoint.id = BINARY mapped_endpoint.endpoint_id
   AND endpoint.provider_id = model.provider_id
WHERE NULLIF(TRIM(mapped_endpoint.endpoint_id), '') IS NOT NULL;

INSERT IGNORE INTO model_endpoint_bindings (
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
    UNIX_TIMESTAMP(),
    UNIX_TIMESTAMP()
FROM models AS model
INNER JOIN JSON_TABLE(
    CASE
        WHEN JSON_VALID(model.provider_model_mappings)
         AND JSON_TYPE(model.provider_model_mappings) = 'ARRAY'
            THEN model.provider_model_mappings
        ELSE JSON_ARRAY()
    END,
    '$[*]' COLUMNS(
        endpoint_ids JSON PATH '$.endpoint_ids',
        api_formats JSON PATH '$.api_formats'
    )
) AS mapping
INNER JOIN JSON_TABLE(
    CASE
        WHEN JSON_VALID(mapping.api_formats)
         AND JSON_TYPE(mapping.api_formats) = 'ARRAY'
            THEN mapping.api_formats
        ELSE JSON_ARRAY()
    END,
    '$[*]' COLUMNS(api_format VARCHAR(128) PATH '$')
) AS mapped_format
INNER JOIN provider_endpoints AS endpoint
    ON endpoint.provider_id = model.provider_id
   AND BINARY LOWER(TRIM(COALESCE(endpoint.api_format, '')))
       = BINARY LOWER(TRIM(mapped_format.api_format))
WHERE NULLIF(TRIM(mapped_format.api_format), '') IS NOT NULL
  AND NOT EXISTS (
      SELECT 1
      FROM JSON_TABLE(
          CASE
              WHEN JSON_VALID(model.provider_model_mappings)
               AND JSON_TYPE(model.provider_model_mappings) = 'ARRAY'
                  THEN model.provider_model_mappings
              ELSE JSON_ARRAY()
          END,
          '$[*]' COLUMNS(endpoint_ids JSON PATH '$.endpoint_ids')
      ) AS mapping
      INNER JOIN JSON_TABLE(
          CASE
              WHEN JSON_VALID(mapping.endpoint_ids)
               AND JSON_TYPE(mapping.endpoint_ids) = 'ARRAY'
                  THEN mapping.endpoint_ids
              ELSE JSON_ARRAY()
          END,
          '$[*]' COLUMNS(endpoint_id VARCHAR(64) PATH '$')
      ) AS mapped_endpoint
      WHERE NULLIF(TRIM(mapped_endpoint.endpoint_id), '') IS NOT NULL
  );

INSERT IGNORE INTO model_endpoint_bindings (
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
    UNIX_TIMESTAMP(),
    UNIX_TIMESTAMP()
FROM models AS model
INNER JOIN provider_endpoints AS endpoint
    ON endpoint.provider_id = model.provider_id
   AND LOWER(TRIM(COALESCE(endpoint.api_format, '')))
       = LOWER(TRIM(COALESCE(model.api_format, '')))
WHERE NULLIF(TRIM(model.api_format), '') IS NOT NULL
  AND NOT EXISTS (
    SELECT 1
    FROM JSON_TABLE(
        CASE
            WHEN JSON_VALID(model.provider_model_mappings)
             AND JSON_TYPE(model.provider_model_mappings) = 'ARRAY'
                THEN model.provider_model_mappings
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(
            endpoint_ids JSON PATH '$.endpoint_ids',
            api_formats JSON PATH '$.api_formats'
        )
    ) AS mapping
    INNER JOIN JSON_TABLE(
        CASE
            WHEN JSON_VALID(mapping.endpoint_ids)
             AND JSON_TYPE(mapping.endpoint_ids) = 'ARRAY'
                THEN mapping.endpoint_ids
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(endpoint_id VARCHAR(64) PATH '$')
    ) AS mapped_endpoint
    WHERE NULLIF(TRIM(mapped_endpoint.endpoint_id), '') IS NOT NULL
)
  AND NOT EXISTS (
    SELECT 1
    FROM JSON_TABLE(
        CASE
            WHEN JSON_VALID(model.provider_model_mappings)
             AND JSON_TYPE(model.provider_model_mappings) = 'ARRAY'
                THEN model.provider_model_mappings
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(api_formats JSON PATH '$.api_formats')
    ) AS mapping
    INNER JOIN JSON_TABLE(
        CASE
            WHEN JSON_VALID(mapping.api_formats)
             AND JSON_TYPE(mapping.api_formats) = 'ARRAY'
                THEN mapping.api_formats
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(api_format VARCHAR(128) PATH '$')
    ) AS mapped_format
    WHERE NULLIF(TRIM(mapped_format.api_format), '') IS NOT NULL
  );

INSERT IGNORE INTO model_endpoint_bindings (
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
    UNIX_TIMESTAMP(),
    UNIX_TIMESTAMP()
FROM models AS model
INNER JOIN provider_endpoints AS endpoint
    ON endpoint.provider_id = model.provider_id
WHERE NOT EXISTS (
    SELECT 1
    FROM JSON_TABLE(
        CASE
            WHEN JSON_VALID(model.provider_model_mappings)
             AND JSON_TYPE(model.provider_model_mappings) = 'ARRAY'
                THEN model.provider_model_mappings
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(endpoint_ids JSON PATH '$.endpoint_ids')
    ) AS mapping
    INNER JOIN JSON_TABLE(
        CASE
            WHEN JSON_VALID(mapping.endpoint_ids)
             AND JSON_TYPE(mapping.endpoint_ids) = 'ARRAY'
                THEN mapping.endpoint_ids
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(endpoint_id VARCHAR(64) PATH '$')
    ) AS mapped_endpoint
    WHERE NULLIF(TRIM(mapped_endpoint.endpoint_id), '') IS NOT NULL
)
  AND NOT EXISTS (
    SELECT 1
    FROM JSON_TABLE(
        CASE
            WHEN JSON_VALID(model.provider_model_mappings)
             AND JSON_TYPE(model.provider_model_mappings) = 'ARRAY'
                THEN model.provider_model_mappings
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(api_formats JSON PATH '$.api_formats')
    ) AS mapping
    INNER JOIN JSON_TABLE(
        CASE
            WHEN JSON_VALID(mapping.api_formats)
             AND JSON_TYPE(mapping.api_formats) = 'ARRAY'
                THEN mapping.api_formats
            ELSE JSON_ARRAY()
        END,
        '$[*]' COLUMNS(api_format VARCHAR(128) PATH '$')
    ) AS mapped_format
    WHERE NULLIF(TRIM(mapped_format.api_format), '') IS NOT NULL
  )
  AND NULLIF(TRIM(model.api_format), '') IS NULL;
