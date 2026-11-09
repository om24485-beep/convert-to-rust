//! The knowledge base: ecosystem packages -> Rust crates, source types -> Rust
//! types, and language concepts -> idiomatic Rust patterns.

use crate::model::{Language, Report};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    WebFramework,
    HttpClient,
    Database,
    Orm,
    Cache,
    Messaging,
    Serialization,
    Validation,
    Auth,
    Logging,
    Config,
    Cli,
    DateTime,
    Testing,
    DataScience,
    MachineLearning,
    Templating,
    Api,
    Networking,
    Cloud,
    Crypto,
    Files,
    Media,
    Scheduling,
    Frontend,
    Desktop,
    Payments,
    Ai,
    Utility,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::WebFramework => "Web framework",
            Category::HttpClient => "HTTP client",
            Category::Database => "Database driver",
            Category::Orm => "ORM",
            Category::Cache => "Cache",
            Category::Messaging => "Messaging / queues",
            Category::Serialization => "Serialization",
            Category::Validation => "Validation",
            Category::Auth => "Auth / security",
            Category::Logging => "Logging",
            Category::Config => "Configuration",
            Category::Cli => "CLI",
            Category::DateTime => "Date & time",
            Category::Testing => "Testing",
            Category::DataScience => "Data science",
            Category::MachineLearning => "Machine learning",
            Category::Templating => "Templating",
            Category::Api => "API (GraphQL/gRPC/OpenAPI)",
            Category::Networking => "Networking / realtime",
            Category::Cloud => "Cloud SDK",
            Category::Crypto => "Crypto",
            Category::Files => "Files & formats",
            Category::Media => "Images & media",
            Category::Scheduling => "Scheduling",
            Category::Frontend => "Frontend UI",
            Category::Desktop => "Desktop",
            Category::Payments => "Payments",
            Category::Ai => "AI / LLM",
            Category::Utility => "Utility",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct CrateRec {
    pub name: &'static str,
    pub version: &'static str,
    pub features: &'static [&'static str],
}

const fn c(name: &'static str, version: &'static str) -> CrateRec {
    CrateRec {
        name,
        version,
        features: &[],
    }
}
const fn cf(
    name: &'static str,
    version: &'static str,
    features: &'static [&'static str],
) -> CrateRec {
    CrateRec {
        name,
        version,
        features,
    }
}

pub struct Rule {
    /// Package names (lower-case). A trailing `*` matches a prefix.
    pub names: &'static [&'static str],
    pub crates: &'static [CrateRec],
    pub category: Category,
    pub note: &'static str,
}

// Frequently reused crate sets.
const AXUM: &[CrateRec] = &[
    c("axum", "0.8"),
    cf("tokio", "1", &["full"]),
    cf("tower-http", "0.6", &["cors", "trace"]),
];
const REQWEST: &[CrateRec] = &[cf("reqwest", "0.12", &["json"])];
const SQLX_PG: &[CrateRec] = &[cf(
    "sqlx",
    "0.8",
    &["runtime-tokio", "tls-rustls", "postgres"],
)];
const SQLX_MYSQL: &[CrateRec] = &[cf("sqlx", "0.8", &["runtime-tokio", "tls-rustls", "mysql"])];
const SQLX_SQLITE: &[CrateRec] = &[cf("sqlx", "0.8", &["runtime-tokio", "sqlite"])];
const SEA_ORM: &[CrateRec] = &[cf(
    "sea-orm",
    "1",
    &["sqlx-postgres", "runtime-tokio-rustls", "macros"],
)];
const TRACING: &[CrateRec] = &[c("tracing", "0.1"), c("tracing-subscriber", "0.3")];
const SERDE_JSON: &[CrateRec] = &[cf("serde", "1", &["derive"]), c("serde_json", "1")];

pub static RULES: &[Rule] = &[
    // --- Web frameworks -------------------------------------------------
    Rule { names: &["express", "koa", "fastify", "hapi", "@hapi/hapi", "restify", "@nestjs/core", "@nestjs/common", "@nestjs/*", "next", "hono"], crates: AXUM, category: Category::WebFramework, note: "Routes become axum handlers; middleware becomes tower layers." },
    Rule { names: &["flask", "django", "djangorestframework", "fastapi", "starlette", "bottle", "falcon", "sanic", "tornado", "aiohttp-server", "quart", "litestar"], crates: AXUM, category: Category::WebFramework, note: "Decorated views become axum handlers; Pydantic/DRF serializers become serde structs." },
    Rule { names: &["spring-boot-starter-web", "spring-boot-starter-webflux", "spring-webmvc", "spring-boot-starter", "quarkus-resteasy*", "micronaut-http*", "javalin", "ktor-server*", "spark-core"], crates: AXUM, category: Category::WebFramework, note: "Controllers become axum routers; DI beans become shared state in `State<Arc<AppState>>`." },
    Rule { names: &["github.com/gin-gonic/gin", "github.com/labstack/echo*", "github.com/gofiber/fiber*", "github.com/go-chi/chi*", "github.com/gorilla/mux", "net/http"], crates: AXUM, category: Category::WebFramework, note: "http.HandlerFunc maps 1:1 to async axum handlers." },
    Rule { names: &["rails", "sinatra", "laravel/framework", "slim/slim", "symfony/*", "microsoft.aspnetcore*"], crates: AXUM, category: Category::WebFramework, note: "Controllers/actions become axum handlers." },
    // --- HTTP clients ----------------------------------------------------
    Rule { names: &["axios", "node-fetch", "got", "superagent", "undici", "ky", "requests", "httpx", "aiohttp", "urllib3", "okhttp", "com.squareup.okhttp3", "feign*", "spring-cloud-starter-openfeign", "github.com/go-resty/resty*", "faraday", "httparty", "guzzlehttp/guzzle", "restsharp"], crates: REQWEST, category: Category::HttpClient, note: "Use one shared `reqwest::Client` (it pools connections)." },
    // --- Databases ---------------------------------------------------------
    Rule { names: &["pg", "postgres", "pg-promise", "psycopg2", "psycopg2-binary", "psycopg", "asyncpg", "postgresql", "github.com/lib/pq", "github.com/jackc/pgx*", "npgsql"], crates: SQLX_PG, category: Category::Database, note: "sqlx checks SQL against the schema at compile time (`query!`)." },
    Rule { names: &["mysql", "mysql2", "pymysql", "mysqlclient", "aiomysql", "mysql-connector-java", "mysql-connector-j", "github.com/go-sql-driver/mysql", "mysql2-gem", "mysqlconnector"], crates: SQLX_MYSQL, category: Category::Database, note: "" },
    Rule { names: &["sqlite3", "better-sqlite3", "aiosqlite", "sqlite-jdbc", "github.com/mattn/go-sqlite3", "microsoft.data.sqlite"], crates: SQLX_SQLITE, category: Category::Database, note: "For sync code, `rusqlite` is a lighter alternative." },
    Rule { names: &["ojdbc*", "oracledb", "cx_oracle", "cx-oracle"], crates: &[c("oracle", "0.6")], category: Category::Database, note: "Oracle needs the Instant Client at runtime; consider `sibyl` for async." },
    Rule { names: &["mssql", "tedious", "pyodbc", "pymssql", "mssql-jdbc", "microsoft.data.sqlclient", "github.com/denisenkom/go-mssqldb"], crates: &[c("tiberius", "0.12")], category: Category::Database, note: "" },
    Rule { names: &["mongoose", "mongodb", "pymongo", "motor", "go.mongodb.org/mongo-driver", "spring-boot-starter-data-mongodb", "mongoid", "mongodb.driver"], crates: &[c("mongodb", "3")], category: Category::Database, note: "Documents map to serde structs with `#[serde(rename = \"_id\")]`." },
    Rule { names: &["cassandra-driver", "scylla", "cassandra-driver-core"], crates: &[c("scylla", "1")], category: Category::Database, note: "" },
    Rule { names: &["@elastic/elasticsearch", "elasticsearch", "elasticsearch-py", "github.com/elastic/go-elasticsearch*"], crates: &[c("elasticsearch", "8.15.0-alpha.1")], category: Category::Database, note: "Or talk to the REST API directly with reqwest." },
    Rule { names: &["sequelize", "typeorm", "prisma", "@prisma/client", "drizzle-orm", "knex", "objection", "mikro-orm", "sqlalchemy", "peewee", "tortoise-orm", "sqlmodel", "hibernate-core", "spring-boot-starter-data-jpa", "spring-data-jpa", "mybatis*", "jooq", "exposed-core", "gorm.io/gorm", "github.com/jmoiron/sqlx", "entgo.io/ent", "activerecord", "sequel", "microsoft.entityframeworkcore*", "dapper", "doctrine/orm", "illuminate/database"], crates: SEA_ORM, category: Category::Orm, note: "SeaORM is closest to ActiveRecord/JPA; Diesel (sync, compile-time) or plain sqlx are alternatives." },
    Rule { names: &["alembic", "flyway-core", "liquibase-core", "knex-migrate", "db-migrate", "github.com/golang-migrate/migrate*"], crates: &[cf("sqlx", "0.8", &["runtime-tokio", "migrate"])], category: Category::Database, note: "`sqlx migrate` covers versioned SQL migrations." },
    // --- Cache / messaging ------------------------------------------------
    Rule { names: &["redis", "ioredis", "redis-py", "aioredis", "github.com/redis/go-redis*", "github.com/go-redis/redis*", "jedis", "lettuce-core", "spring-boot-starter-data-redis", "stackexchange.redis", "predis/predis"], crates: &[cf("redis", "0.27", &["tokio-comp"])], category: Category::Cache, note: "" },
    Rule { names: &["node-cache", "lru-cache", "cachetools", "caffeine", "guava", "github.com/patrickmn/go-cache", "memory-cache"], crates: &[cf("moka", "0.12", &["future"])], category: Category::Cache, note: "moka is a concurrent TinyLFU cache (Caffeine-like)." },
    Rule { names: &["kafkajs", "kafka-node", "kafka-python", "confluent-kafka", "aiokafka", "spring-kafka", "kafka-clients", "github.com/segmentio/kafka-go", "github.com/confluentinc/confluent-kafka-go*", "github.com/shopify/sarama", "github.com/ibm/sarama", "confluent.kafka"], crates: &[c("rdkafka", "0.36")], category: Category::Messaging, note: "rdkafka wraps librdkafka (needs cmake to build)." },
    Rule { names: &["amqplib", "amqp-connection-manager", "pika", "aio-pika", "kombu", "spring-boot-starter-amqp", "amqp-client", "github.com/rabbitmq/amqp091-go", "github.com/streadway/amqp", "bunny", "rabbitmq.client"], crates: &[c("lapin", "2")], category: Category::Messaging, note: "" },
    Rule { names: &["celery", "rq", "dramatiq", "huey", "bull", "bullmq", "bee-queue", "agenda", "sidekiq", "resque", "github.com/hibiken/asynq", "hangfire*"], crates: &[cf("tokio", "1", &["full"]), cf("redis", "0.27", &["tokio-comp"])], category: Category::Messaging, note: "Background jobs: tokio tasks for in-process work; a Redis/RabbitMQ-backed worker (e.g. the `apalis` crate) for durable queues." },
    Rule { names: &["nats", "nats.py", "github.com/nats-io/nats.go"], crates: &[c("async-nats", "0.38")], category: Category::Messaging, note: "" },
    Rule { names: &["@aws-sdk/client-sqs", "aws-sdk-sqs"], crates: &[c("aws-sdk-sqs", "1"), c("aws-config", "1")], category: Category::Messaging, note: "" },
    // --- Serialization / validation ----------------------------------------
    Rule { names: &["pydantic", "marshmallow", "attrs", "dataclasses-json", "msgspec", "class-transformer", "jackson-databind", "jackson-core", "com.fasterxml.jackson*", "gson", "moshi", "kotlinx-serialization-json", "newtonsoft.json", "system.text.json", "github.com/json-iterator/go", "encoding/json", "orjson", "ujson", "simplejson", "json5", "superjson"], crates: SERDE_JSON, category: Category::Serialization, note: "`#[derive(Serialize, Deserialize)]` replaces model/serializer classes." },
    Rule { names: &["pyyaml", "yaml", "ruamel.yaml", "js-yaml", "snakeyaml", "gopkg.in/yaml*", "yamldotnet"], crates: &[c("serde_yaml", "0.9")], category: Category::Serialization, note: "" },
    Rule { names: &["toml", "tomli", "tomlkit", "@iarna/toml", "github.com/burntsushi/toml", "github.com/pelletier/go-toml*"], crates: &[c("toml", "0.8")], category: Category::Serialization, note: "" },
    Rule { names: &["xml2js", "fast-xml-parser", "lxml", "xmltodict", "jaxb-api", "jakarta.xml.bind-api", "nokogiri", "beevik/etree", "encoding/xml"], crates: &[cf("quick-xml", "0.36", &["serialize"])], category: Category::Serialization, note: "" },
    Rule { names: &["protobufjs", "google-protobuf", "protobuf", "protobuf-java", "google.golang.org/protobuf", "github.com/golang/protobuf", "google.protobuf"], crates: &[c("prost", "0.13")], category: Category::Serialization, note: "" },
    Rule { names: &["msgpack", "msgpack-lite", "@msgpack/msgpack", "github.com/vmihailenco/msgpack*"], crates: &[c("rmp-serde", "1")], category: Category::Serialization, note: "" },
    Rule { names: &["joi", "zod", "yup", "class-validator", "express-validator", "ajv", "validator", "cerberus", "voluptuous", "jsonschema", "hibernate-validator", "spring-boot-starter-validation", "jakarta.validation-api", "github.com/go-playground/validator*", "fluentvalidation"], crates: &[cf("validator", "0.18", &["derive"])], category: Category::Validation, note: "`#[derive(Validate)]` with `#[validate(email, length(min = 1))]` attributes." },
    // --- Auth / security ----------------------------------------------------
    Rule { names: &["jsonwebtoken", "jose", "jwt-decode", "express-jwt", "passport-jwt", "pyjwt", "python-jose", "authlib", "jjwt*", "java-jwt", "nimbus-jose-jwt", "github.com/golang-jwt/jwt*", "github.com/dgrijalva/jwt-go", "jwt", "system.identitymodel.tokens.jwt", "firebase/php-jwt"], crates: &[c("jsonwebtoken", "9")], category: Category::Auth, note: "" },
    Rule { names: &["bcrypt", "bcryptjs", "passlib", "flask-bcrypt", "spring-security-crypto", "golang.org/x/crypto", "bcrypt-ruby", "bcrypt.net-next"], crates: &[c("bcrypt", "0.15")], category: Category::Auth, note: "For new hashes prefer argon2." },
    Rule { names: &["argon2", "argon2-cffi", "argon2-jvm", "argon2id"], crates: &[c("argon2", "0.5")], category: Category::Auth, note: "" },
    Rule { names: &["passport", "passport-local", "next-auth", "flask-login", "django-allauth", "flask-jwt-extended", "spring-boot-starter-security", "spring-security-*", "devise", "omniauth", "github.com/markbates/goth"], crates: &[c("axum-login", "0.16"), c("tower-sessions", "0.13")], category: Category::Auth, note: "Session auth: axum-login + tower-sessions. OAuth2: the `oauth2` crate." },
    Rule { names: &["oauthlib", "requests-oauthlib", "simple-oauth2", "golang.org/x/oauth2", "spring-boot-starter-oauth2-client", "openid-client"], crates: &[c("oauth2", "4")], category: Category::Auth, note: "" },
    Rule { names: &["helmet", "cors", "flask-cors", "django-cors-headers", "github.com/rs/cors"], crates: &[cf("tower-http", "0.6", &["cors", "set-header"])], category: Category::WebFramework, note: "CORS and security headers are tower-http layers." },
    Rule { names: &["express-rate-limit", "rate-limiter-flexible", "slowapi", "flask-limiter", "bucket4j*", "resilience4j-ratelimiter", "golang.org/x/time", "github.com/ulule/limiter*", "rack-attack"], crates: &[c("governor", "0.7"), c("tower_governor", "0.5")], category: Category::WebFramework, note: "" },
    Rule { names: &["crypto-js", "node-forge", "cryptography", "pycryptodome", "pycryptodomex", "pynacl", "bouncycastle*", "bcprov*", "tweetnacl", "libsodium-wrappers"], crates: &[c("sha2", "0.10"), c("aes-gcm", "0.10"), c("rand", "0.8")], category: Category::Crypto, note: "RustCrypto crates; use `ring` or `rustls` for TLS-level primitives." },
    Rule { names: &["uuid", "uuidv4", "nanoid", "shortuuid", "github.com/google/uuid", "github.com/gofrs/uuid", "java-uuid-generator"], crates: &[cf("uuid", "1", &["v4", "serde"])], category: Category::Utility, note: "" },
    // --- Logging / config -----------------------------------------------------
    Rule { names: &["winston", "pino", "pino-http", "morgan", "bunyan", "log4js", "debug", "loguru", "structlog", "logbook", "logback-classic", "logback-core", "slf4j-api", "log4j-core", "log4j-api", "spring-boot-starter-logging", "github.com/sirupsen/logrus", "go.uber.org/zap", "github.com/rs/zerolog", "log/slog", "serilog*", "nlog*", "microsoft.extensions.logging*", "lograge", "monolog/monolog"], crates: TRACING, category: Category::Logging, note: "tracing gives structured, span-aware logs (like MDC/context loggers)." },
    Rule { names: &["sentry", "@sentry/node", "sentry-sdk", "sentry-java", "github.com/getsentry/sentry-go", "sentry-ruby"], crates: &[c("sentry", "0.34")], category: Category::Logging, note: "" },
    Rule { names: &["prom-client", "prometheus-client", "prometheus_client", "micrometer-core", "micrometer-registry-prometheus", "github.com/prometheus/client_golang"], crates: &[c("metrics", "0.24"), c("metrics-exporter-prometheus", "0.16")], category: Category::Logging, note: "" },
    Rule { names: &["@opentelemetry/api", "@opentelemetry/sdk-node", "opentelemetry-api", "opentelemetry-sdk", "go.opentelemetry.io/otel*"], crates: &[c("opentelemetry", "0.27"), c("tracing-opentelemetry", "0.28")], category: Category::Logging, note: "" },
    Rule { names: &["dotenv", "dotenv-flow", "python-dotenv", "github.com/joho/godotenv", "dotenv-java", "dotenv-rails", "vlucas/phpdotenv", "dotnetenv"], crates: &[c("dotenvy", "0.15")], category: Category::Config, note: "" },
    Rule { names: &["config", "convict", "nconf", "rc", "pydantic-settings", "dynaconf", "python-decouple", "hydra-core", "spring-boot-configuration-processor", "github.com/spf13/viper", "github.com/kelseyhightower/envconfig", "typesafe-config", "microsoft.extensions.configuration*"], crates: &[c("config", "0.14")], category: Category::Config, note: "Layer files + env vars into a typed `Settings` struct via serde." },
    // --- CLI --------------------------------------------------------------------
    Rule { names: &["commander", "yargs", "meow", "minimist", "oclif", "@oclif/core", "click", "typer", "docopt", "fire", "argparse", "github.com/spf13/cobra", "github.com/urfave/cli*", "flag", "picocli", "jcommander", "thor", "commandlineparser", "system.commandline", "symfony/console"], crates: &[cf("clap", "4", &["derive"])], category: Category::Cli, note: "`#[derive(Parser)]` turns argument definitions into a struct." },
    Rule { names: &["inquirer", "prompts", "@inquirer/prompts", "questionary", "pyinquirer", "github.com/manifoldco/promptui", "github.com/alecaivazis/survey*", "tty-prompt"], crates: &[c("dialoguer", "0.11")], category: Category::Cli, note: "" },
    Rule { names: &["chalk", "colors", "kleur", "colorama", "termcolor", "rich", "github.com/fatih/color", "jansi", "colorize", "pastel"], crates: &[c("owo-colors", "4")], category: Category::Cli, note: "" },
    Rule { names: &["ora", "cli-progress", "progress", "tqdm", "alive-progress", "github.com/schollz/progressbar*", "github.com/cheggaaa/pb*", "ruby-progressbar"], crates: &[c("indicatif", "0.17")], category: Category::Cli, note: "" },
    Rule { names: &["blessed", "ink", "textual", "urwid", "curses", "github.com/charmbracelet/bubbletea", "github.com/rivo/tview", "lanterna"], crates: &[c("ratatui", "0.29"), c("crossterm", "0.28")], category: Category::Cli, note: "" },
    // --- Date / time ---------------------------------------------------------------
    Rule { names: &["moment", "moment-timezone", "dayjs", "date-fns", "luxon", "python-dateutil", "dateutil", "arrow", "pendulum", "pytz", "tzdata", "joda-time", "threetenbp", "nodatime", "carbon", "nesbot/carbon", "github.com/jinzhu/now"], crates: &[cf("chrono", "0.4", &["serde"])], category: Category::DateTime, note: "chrono (+ chrono-tz for zones); `jiff` is a newer alternative." },
    // --- Testing ----------------------------------------------------------------------
    Rule { names: &["jest", "mocha", "vitest", "jasmine", "ava", "tap", "chai", "sinon", "@testing-library/*", "pytest", "pytest-*", "nose", "nose2", "unittest2", "junit", "junit-jupiter*", "org.junit*", "testng", "mockito*", "assertj-core", "spring-boot-starter-test", "kotest*", "mockk", "github.com/stretchr/testify", "github.com/onsi/ginkgo*", "github.com/onsi/gomega", "github.com/golang/mock", "go.uber.org/mock", "rspec", "rspec-*", "minitest", "phpunit/phpunit", "pestphp/pest", "xunit*", "nunit*", "moq", "fluentassertions", "factory_bot", "faker", "@faker-js/faker", "hypothesis", "fast-check"], crates: &[c("mockall", "0.13"), c("proptest", "1")], category: Category::Testing, note: "Unit tests live next to code in `#[cfg(test)] mod tests`; integration tests in `tests/`." },
    Rule { names: &["supertest", "nock", "msw", "responses", "requests-mock", "httpretty", "vcrpy", "wiremock*", "mockwebserver", "github.com/jarcoal/httpmock", "webmock"], crates: &[c("wiremock", "0.6")], category: Category::Testing, note: "Use `tower::ServiceExt::oneshot` to test axum routers without a socket." },
    Rule { names: &["benchmark", "pytest-benchmark", "jmh-core", "benchmarkdotnet"], crates: &[c("criterion", "0.5")], category: Category::Testing, note: "" },
    Rule { names: &["cypress", "playwright", "@playwright/test", "selenium", "selenium-java", "puppeteer", "webdriverio", "capybara"], crates: &[c("fantoccini", "0.21")], category: Category::Testing, note: "Browser E2E suites can stay in their current language; they test behaviour, not implementation." },
    // --- Data / ML ----------------------------------------------------------------------
    Rule { names: &["pandas", "polars", "pyspark", "dask", "modin", "danfojs", "arquero", "tablesaw*", "spark-sql*"], crates: &[cf("polars", "0.46", &["lazy"])], category: Category::DataScience, note: "Polars' lazy API is the closest pandas/Spark equivalent." },
    Rule { names: &["numpy", "mathjs", "ndarray", "numjs", "gonum.org/v1/gonum", "nd4j*", "numsharp", "ejml*"], crates: &[c("ndarray", "0.16")], category: Category::DataScience, note: "nalgebra for linear algebra on fixed-size matrices." },
    Rule { names: &["scipy", "statsmodels", "simple-statistics", "commons-math3", "apache-commons-math"], crates: &[c("statrs", "0.18"), c("ndarray", "0.16")], category: Category::DataScience, note: "" },
    Rule { names: &["scikit-learn", "sklearn", "xgboost", "lightgbm", "catboost", "smile-core", "weka*", "ml.net", "microsoft.ml"], crates: &[c("linfa", "0.7"), c("smartcore", "0.4")], category: Category::MachineLearning, note: "For trained models, export to ONNX and serve with the `ort` crate." },
    Rule { names: &["torch", "pytorch", "tensorflow", "keras", "jax", "transformers", "onnxruntime", "@tensorflow/tfjs*", "deeplearning4j*", "djl*"], crates: &[c("candle-core", "0.8"), c("candle-nn", "0.8")], category: Category::MachineLearning, note: "Inference: candle or ONNX Runtime (`ort`); training usually stays in Python." },
    Rule { names: &["matplotlib", "seaborn", "plotly", "bokeh", "altair", "chart.js", "d3", "jfreechart", "github.com/wcharczuk/go-chart*", "gonum.org/v1/plot"], crates: &[c("plotters", "0.3")], category: Category::DataScience, note: "" },
    Rule { names: &["openai", "@openai/openai", "langchain", "@langchain/*", "langchain-*", "llama-index", "llama_index", "github.com/sashabaranov/go-openai", "openai-java", "ruby-openai", "openai-php/client"], crates: &[c("async-openai", "0.26")], category: Category::Ai, note: "For other providers call the REST API with reqwest + serde." },
    Rule { names: &["anthropic", "@anthropic-ai/sdk", "anthropic-java", "github.com/anthropics/anthropic-sdk-go"], crates: &[cf("reqwest", "0.12", &["json"]), cf("serde", "1", &["derive"]), c("serde_json", "1")], category: Category::Ai, note: "Call the Messages API with reqwest; the request/response shapes are small serde structs." },
    // --- Templating / API styles -----------------------------------------------------
    Rule { names: &["ejs", "pug", "jade", "handlebars", "hbs", "express-handlebars", "nunjucks", "mustache", "jinja2", "mako", "chameleon", "thymeleaf", "spring-boot-starter-thymeleaf", "freemarker", "velocity*", "html/template", "text/template", "erb", "haml", "slim", "twig/twig", "liquid", "razorlight"], crates: &[c("askama", "0.12")], category: Category::Templating, note: "askama compiles templates at build time; minijinja is runtime Jinja2-compatible." },
    Rule { names: &["graphql", "apollo-server", "@apollo/server", "apollo-server-express", "graphql-yoga", "type-graphql", "@nestjs/graphql", "graphene", "graphene-django", "strawberry-graphql", "ariadne", "graphql-java", "spring-boot-starter-graphql", "github.com/99designs/gqlgen", "github.com/graphql-go/graphql", "graphql-ruby", "hotchocolate*"], crates: &[c("async-graphql", "7"), c("async-graphql-axum", "7")], category: Category::Api, note: "" },
    Rule { names: &["@grpc/grpc-js", "grpc", "@grpc/proto-loader", "grpcio", "grpcio-tools", "grpc-java", "io.grpc*", "grpc-netty*", "google.golang.org/grpc", "grpc.aspnetcore", "grpc.net.client"], crates: &[c("tonic", "0.12"), c("prost", "0.13")], category: Category::Api, note: "Reuse the existing .proto files with tonic-build." },
    Rule { names: &["swagger-ui-express", "swagger-jsdoc", "@nestjs/swagger", "drf-yasg", "drf-spectacular", "flasgger", "flask-restx", "springdoc-openapi*", "springfox*", "github.com/swaggo/swag", "swashbuckle*", "rswag"], crates: &[cf("utoipa", "5", &["axum_extras"]), cf("utoipa-swagger-ui", "8", &["axum"])], category: Category::Api, note: "" },
    // --- Networking / realtime --------------------------------------------------------
    Rule { names: &["ws", "websockets", "websocket-client", "python-socketio", "github.com/gorilla/websocket", "nhooyr.io/websocket", "spring-boot-starter-websocket", "faye-websocket", "actioncable"], crates: &[c("tokio-tungstenite", "0.24"), cf("axum", "0.8", &["ws"])], category: Category::Networking, note: "axum has built-in WebSocket upgrade support." },
    Rule { names: &["socket.io", "socket.io-client", "flask-socketio", "netty-socketio", "github.com/googollee/go-socket.io"], crates: &[c("socketioxide", "0.15")], category: Category::Networking, note: "" },
    Rule { names: &["nodemailer", "sendgrid", "@sendgrid/mail", "flask-mail", "django-anymail", "spring-boot-starter-mail", "javax.mail", "jakarta.mail", "gopkg.in/gomail.v2", "mail", "actionmailer", "phpmailer/phpmailer", "mailkit"], crates: &[cf("lettre", "0.11", &["tokio1-native-tls"])], category: Category::Networking, note: "" },
    Rule { names: &["twilio", "twilio-node", "twilio-java", "github.com/twilio/twilio-go"], crates: REQWEST, category: Category::Networking, note: "Call the provider's REST API with reqwest." },
    Rule { names: &["cheerio", "jsdom", "beautifulsoup4", "bs4", "scrapy", "parsel", "jsoup", "github.com/puerkitobio/goquery", "nokogiri-html", "htmlagilitypack", "anglesharp"], crates: &[c("scraper", "0.20")], category: Category::Networking, note: "Headless browsers: chromiumoxide or fantoccini." },
    // --- Cloud --------------------------------------------------------------------------
    Rule { names: &["aws-sdk", "@aws-sdk/client-s3", "@aws-sdk/*", "boto3", "botocore", "aioboto3", "aws-java-sdk*", "software.amazon.awssdk*", "github.com/aws/aws-sdk-go*", "aws-sdk-s3", "awssdk.s3"], crates: &[c("aws-config", "1"), c("aws-sdk-s3", "1")], category: Category::Cloud, note: "The official AWS SDK for Rust has one crate per service (aws-sdk-dynamodb, aws-sdk-sqs, ...)." },
    Rule { names: &["@google-cloud/storage", "google-cloud-storage", "google-cloud-*", "@google-cloud/*", "cloud.google.com/go*", "firebase-admin"], crates: &[c("google-cloud-storage", "0.24")], category: Category::Cloud, note: "" },
    Rule { names: &["@azure/storage-blob", "azure-storage-blob", "azure-*", "@azure/*", "azure.storage.blobs", "github.com/azure/azure-sdk-for-go*"], crates: &[c("azure_storage_blobs", "0.21"), c("azure_identity", "0.21")], category: Category::Cloud, note: "" },
    Rule { names: &["minio"], crates: &[c("aws-sdk-s3", "1")], category: Category::Cloud, note: "MinIO is S3-compatible." },
    Rule { names: &["stripe", "stripe-java", "github.com/stripe/stripe-go*", "stripe/stripe-php", "stripe.net"], crates: &[c("async-stripe", "0.39")], category: Category::Payments, note: "" },
    Rule { names: &["razorpay"], crates: REQWEST, category: Category::Payments, note: "No official crate: call the REST API with reqwest + serde." },
    // --- Files / media ---------------------------------------------------------------------
    Rule { names: &["csv-parser", "csv-parse", "papaparse", "fast-csv", "csv", "unicodecsv", "opencsv", "commons-csv", "encoding/csv", "csvhelper"], crates: &[c("csv", "1")], category: Category::Files, note: "" },
    Rule { names: &["xlsx", "exceljs", "sheetjs", "openpyxl", "xlrd", "xlsxwriter", "pyexcel", "poi", "poi-ooxml", "apache-poi", "github.com/xuri/excelize*", "closedxml", "epplus", "npoi", "phpoffice/phpspreadsheet"], crates: &[c("calamine", "0.26"), c("rust_xlsxwriter", "0.79")], category: Category::Files, note: "calamine reads, rust_xlsxwriter writes." },
    Rule { names: &["pdfkit", "pdf-lib", "jspdf", "pdfmake", "pdf-parse", "reportlab", "pypdf", "pypdf2", "pdfplumber", "pdfminer.six", "weasyprint", "itext*", "pdfbox", "openpdf", "github.com/jung-kurt/gofpdf", "prawn", "dompdf/dompdf", "tecnickcom/tcpdf", "questpdf", "itextsharp"], crates: &[c("lopdf", "0.34"), c("printpdf", "0.7")], category: Category::Files, note: "lopdf reads/edits, printpdf generates; `typst` is great for templated documents." },
    Rule { names: &["docx", "python-docx", "officegen", "docxtemplater"], crates: &[c("docx-rs", "0.4")], category: Category::Files, note: "" },
    Rule { names: &["archiver", "adm-zip", "jszip", "yauzl", "zipfile36", "commons-compress", "archive/zip"], crates: &[c("zip", "2")], category: Category::Files, note: "" },
    Rule { names: &["multer", "formidable", "busboy", "express-fileupload", "python-multipart", "werkzeug", "commons-fileupload"], crates: &[cf("axum", "0.8", &["multipart"])], category: Category::WebFramework, note: "axum's `Multipart` extractor handles uploads." },
    Rule { names: &["chokidar", "watchdog", "github.com/fsnotify/fsnotify", "filewatcher"], crates: &[c("notify", "7")], category: Category::Files, note: "" },
    Rule { names: &["glob", "fast-glob", "globby", "glob2", "wcmatch", "github.com/bmatcuk/doublestar*"], crates: &[c("glob", "0.3"), c("walkdir", "2")], category: Category::Files, note: "" },
    Rule { names: &["sharp", "jimp", "pillow", "pil", "opencv-python", "cv2", "imageio", "scikit-image", "wand", "thumbnailator", "imgscalr", "github.com/disintegration/imaging", "mini_magick", "imagesharp", "skiasharp", "intervention/image"], crates: &[c("image", "0.25")], category: Category::Media, note: "OpenCV workloads: the `opencv` crate binds the C++ library." },
    Rule { names: &["fluent-ffmpeg", "ffmpeg-python", "moviepy", "javacv"], crates: &[c("ffmpeg-next", "7")], category: Category::Media, note: "Or shell out to the ffmpeg CLI with std::process::Command." },
    Rule { names: &["qrcode", "qrcode-generator", "zxing*", "github.com/skip2/go-qrcode", "rqrcode", "qrcoder"], crates: &[c("qrcode", "0.14")], category: Category::Media, note: "" },
    // --- Scheduling / concurrency ----------------------------------------------------------
    Rule { names: &["node-cron", "cron", "node-schedule", "apscheduler", "schedule", "celery-beat", "django-crontab", "quartz", "spring-boot-starter-quartz", "github.com/robfig/cron*", "github.com/go-co-op/gocron*", "whenever", "rufus-scheduler", "quartz.net"], crates: &[c("tokio-cron-scheduler", "0.13")], category: Category::Scheduling, note: "Simple intervals: `tokio::time::interval`." },
    Rule { names: &["rxjs", "rxjava", "io.reactivex*", "reactor-core", "projectreactor*", "rx.net", "system.reactive", "kotlinx-coroutines-core"], crates: &[c("futures", "0.3"), c("tokio-stream", "0.1")], category: Category::Utility, note: "Observables/Flux become `Stream`s; operators map to StreamExt combinators." },
    Rule { names: &["p-limit", "p-queue", "async", "bluebird", "q", "gevent", "eventlet", "trio", "anyio", "golang.org/x/sync"], crates: &[cf("tokio", "1", &["full"]), c("futures", "0.3")], category: Category::Utility, note: "" },
    Rule { names: &["joblib", "multiprocess", "ray", "parallel", "github.com/sourcegraph/conc"], crates: &[c("rayon", "1")], category: Category::Utility, note: "rayon parallelises CPU-bound iterators with `.par_iter()`." },
    // --- Frontend / desktop --------------------------------------------------------------
    Rule { names: &["react", "react-dom", "vue", "svelte", "@angular/core", "preact", "solid-js", "lit", "alpinejs", "jquery"], crates: &[c("leptos", "0.7")], category: Category::Frontend, note: "Usually keep the frontend in TypeScript and port only the backend; full-Rust options are Leptos, Dioxus or Yew (WASM)." },
    Rule { names: &["electron", "electron-builder", "nw", "neutralinojs", "pyqt5", "pyqt6", "pyside2", "pyside6", "tkinter", "wxpython", "kivy", "javafx*", "swing", "github.com/wailsapp/wails*", "fyne.io/fyne*", "avalonia*", "wpf", "winforms"], crates: &[c("tauri", "2")], category: Category::Desktop, note: "Tauri keeps a web UI with a Rust backend; egui/iced/slint for native Rust UIs." },
    Rule { names: &["react-native", "expo", "flutter"], crates: &[c("dioxus", "0.6")], category: Category::Frontend, note: "Mobile: keep the UI, move shared logic into a Rust library via UniFFI." },
    // --- Utilities ------------------------------------------------------------------------
    Rule { names: &["lodash", "underscore", "ramda", "immutable", "toolz", "more-itertools", "cytoolz", "funcy", "commons-lang3", "commons-collections4", "vavr", "github.com/samber/lo", "activesupport", "morelinq"], crates: &[c("itertools", "0.13")], category: Category::Utility, note: "Most helpers are iterator adaptors in std + itertools." },
    Rule { names: &["decimal.js", "big.js", "bignumber.js", "decimal", "py-moneyed", "money", "joda-money", "javamoney*", "github.com/shopspring/decimal", "bigdecimal", "brick/money", "moneyphp/money"], crates: &[cf("rust_decimal", "1", &["serde"])], category: Category::Utility, note: "Never use f64 for money: rust_decimal is the BigDecimal equivalent." },
    Rule { names: &["semver", "packaging", "github.com/masterminds/semver*", "github.com/hashicorp/go-version"], crates: &[c("semver", "1")], category: Category::Utility, note: "" },
    Rule { names: &["chardet", "iconv-lite", "charset-normalizer"], crates: &[c("encoding_rs", "0.8")], category: Category::Utility, note: "" },
    Rule { names: &["slugify", "python-slugify", "slug", "github.com/gosimple/slug"], crates: &[c("slug", "0.1")], category: Category::Utility, note: "" },
    Rule { names: &["handlebars-helpers", "marked", "markdown-it", "markdown", "mistune", "commonmark", "flexmark*", "github.com/yuin/goldmark", "redcarpet", "kramdown", "league/commonmark", "markdig"], crates: &[c("pulldown-cmark", "0.12")], category: Category::Utility, note: "" },
    Rule { names: &["regex", "re2", "xregexp", "github.com/dlclark/regexp2"], crates: &[c("regex", "1")], category: Category::Utility, note: "" },
    Rule { names: &["base64", "js-base64", "base64-js"], crates: &[c("base64", "0.22")], category: Category::Utility, note: "" },
    Rule { names: &["sanitize-html", "dompurify", "isomorphic-dompurify", "bleach", "owasp-java-html-sanitizer", "github.com/microcosm-cc/bluemonday", "htmlsanitizer"], crates: &[c("ammonia", "4")], category: Category::Auth, note: "" },
    Rule { names: &["i18next", "react-intl", "babel", "flask-babel", "gettext", "go-i18n", "rails-i18n"], crates: &[c("fluent", "0.16")], category: Category::Utility, note: "" },
];

/// Standard-library modules that have a well-known Rust counterpart, keyed by
/// (language, import prefix).
pub struct StdHint {
    pub lang: Language,
    pub module: &'static str,
    pub rust: &'static str,
    pub crates: &'static [CrateRec],
}

pub static STD_HINTS: &[StdHint] = &[
    StdHint {
        lang: Language::Python,
        module: "asyncio",
        rust: "tokio runtime, async/.await, tokio::spawn",
        crates: &[cf("tokio", "1", &["full"])],
    },
    StdHint {
        lang: Language::Python,
        module: "json",
        rust: "serde_json",
        crates: SERDE_JSON,
    },
    StdHint {
        lang: Language::Python,
        module: "re",
        rust: "regex crate",
        crates: &[c("regex", "1")],
    },
    StdHint {
        lang: Language::Python,
        module: "datetime",
        rust: "chrono",
        crates: &[cf("chrono", "0.4", &["serde"])],
    },
    StdHint {
        lang: Language::Python,
        module: "decimal",
        rust: "rust_decimal",
        crates: &[cf("rust_decimal", "1", &["serde"])],
    },
    StdHint {
        lang: Language::Python,
        module: "logging",
        rust: "tracing",
        crates: TRACING,
    },
    StdHint {
        lang: Language::Python,
        module: "argparse",
        rust: "clap derive",
        crates: &[cf("clap", "4", &["derive"])],
    },
    StdHint {
        lang: Language::Python,
        module: "sqlite3",
        rust: "rusqlite or sqlx (sqlite)",
        crates: &[cf("rusqlite", "0.32", &["bundled"])],
    },
    StdHint {
        lang: Language::Python,
        module: "csv",
        rust: "csv crate",
        crates: &[c("csv", "1")],
    },
    StdHint {
        lang: Language::Python,
        module: "uuid",
        rust: "uuid crate",
        crates: &[cf("uuid", "1", &["v4", "serde"])],
    },
    StdHint {
        lang: Language::Python,
        module: "hashlib",
        rust: "sha2 / md-5",
        crates: &[c("sha2", "0.10")],
    },
    StdHint {
        lang: Language::Python,
        module: "random",
        rust: "rand",
        crates: &[c("rand", "0.8")],
    },
    StdHint {
        lang: Language::Python,
        module: "subprocess",
        rust: "std::process::Command",
        crates: &[],
    },
    StdHint {
        lang: Language::Python,
        module: "threading",
        rust: "std::thread + Arc<Mutex<T>> (or tokio tasks)",
        crates: &[],
    },
    StdHint {
        lang: Language::Python,
        module: "os",
        rust: "std::env, std::fs, std::path",
        crates: &[],
    },
    StdHint {
        lang: Language::Python,
        module: "pathlib",
        rust: "std::path::{Path, PathBuf}",
        crates: &[],
    },
    StdHint {
        lang: Language::Python,
        module: "dataclasses",
        rust: "plain structs + #[derive(...)]",
        crates: &[],
    },
    StdHint {
        lang: Language::Python,
        module: "typing",
        rust: "native generics, Option<T>, enums",
        crates: &[],
    },
    StdHint {
        lang: Language::Python,
        module: "enum",
        rust: "enum",
        crates: &[],
    },
    StdHint {
        lang: Language::Python,
        module: "http.server",
        rust: "axum",
        crates: AXUM,
    },
    StdHint {
        lang: Language::Python,
        module: "urllib",
        rust: "reqwest + url",
        crates: REQWEST,
    },
    StdHint {
        lang: Language::JavaScript,
        module: "fs",
        rust: "std::fs / tokio::fs",
        crates: &[],
    },
    StdHint {
        lang: Language::JavaScript,
        module: "path",
        rust: "std::path",
        crates: &[],
    },
    StdHint {
        lang: Language::JavaScript,
        module: "http",
        rust: "axum (server) / reqwest (client)",
        crates: AXUM,
    },
    StdHint {
        lang: Language::JavaScript,
        module: "crypto",
        rust: "sha2, hmac, rand",
        crates: &[c("sha2", "0.10"), c("rand", "0.8")],
    },
    StdHint {
        lang: Language::JavaScript,
        module: "events",
        rust: "tokio::sync::broadcast / mpsc channels",
        crates: &[],
    },
    StdHint {
        lang: Language::JavaScript,
        module: "child_process",
        rust: "std::process::Command / tokio::process",
        crates: &[],
    },
    StdHint {
        lang: Language::Go,
        module: "net/http",
        rust: "axum + reqwest",
        crates: AXUM,
    },
    StdHint {
        lang: Language::Go,
        module: "encoding/json",
        rust: "serde_json",
        crates: SERDE_JSON,
    },
    StdHint {
        lang: Language::Go,
        module: "database/sql",
        rust: "sqlx",
        crates: SQLX_PG,
    },
    StdHint {
        lang: Language::Go,
        module: "context",
        rust: "tokio cancellation (CancellationToken) / timeouts",
        crates: &[c("tokio-util", "0.7")],
    },
    StdHint {
        lang: Language::Go,
        module: "sync",
        rust: "std::sync / tokio::sync",
        crates: &[],
    },
    StdHint {
        lang: Language::Go,
        module: "time",
        rust: "std::time + chrono",
        crates: &[cf("chrono", "0.4", &["serde"])],
    },
    StdHint {
        lang: Language::Go,
        module: "log",
        rust: "tracing",
        crates: TRACING,
    },
    StdHint {
        lang: Language::Go,
        module: "flag",
        rust: "clap",
        crates: &[cf("clap", "4", &["derive"])],
    },
    StdHint {
        lang: Language::Java,
        module: "java.util.concurrent",
        rust: "tokio / rayon / std::sync",
        crates: &[cf("tokio", "1", &["full"])],
    },
    StdHint {
        lang: Language::Java,
        module: "java.math.BigDecimal",
        rust: "rust_decimal::Decimal",
        crates: &[cf("rust_decimal", "1", &["serde"])],
    },
    StdHint {
        lang: Language::Java,
        module: "java.time",
        rust: "chrono",
        crates: &[cf("chrono", "0.4", &["serde"])],
    },
    StdHint {
        lang: Language::Java,
        module: "java.util.UUID",
        rust: "uuid",
        crates: &[cf("uuid", "1", &["v4", "serde"])],
    },
    StdHint {
        lang: Language::Java,
        module: "java.sql",
        rust: "sqlx",
        crates: SQLX_PG,
    },
    StdHint {
        lang: Language::Java,
        module: "java.net.http",
        rust: "reqwest",
        crates: REQWEST,
    },
    StdHint {
        lang: Language::CSharp,
        module: "System.Text.Json",
        rust: "serde_json",
        crates: SERDE_JSON,
    },
    StdHint {
        lang: Language::CSharp,
        module: "System.Net.Http",
        rust: "reqwest",
        crates: REQWEST,
    },
    StdHint {
        lang: Language::CSharp,
        module: "System.Threading.Tasks",
        rust: "tokio",
        crates: &[cf("tokio", "1", &["full"])],
    },
    StdHint {
        lang: Language::CSharp,
        module: "System.Linq",
        rust: "Iterator adaptors (map/filter/fold)",
        crates: &[],
    },
    StdHint {
        lang: Language::C,
        module: "pthread.h",
        rust: "std::thread + std::sync",
        crates: &[],
    },
    StdHint {
        lang: Language::C,
        module: "stdio.h",
        rust: "std::io / println!",
        crates: &[],
    },
    StdHint {
        lang: Language::C,
        module: "stdlib.h",
        rust: "Box/Vec ownership (no malloc/free)",
        crates: &[],
    },
    StdHint {
        lang: Language::C,
        module: "string.h",
        rust: "String / &str / slices",
        crates: &[],
    },
];

pub fn normalize(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('_', "-")
}

fn matches(pattern: &str, name: &str) -> bool {
    let pattern = normalize(pattern);
    let name = normalize(name);
    if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        pattern == name
    }
}

/// Find the rule for a package name (and optional Maven group).
pub fn lookup(name: &str, group: Option<&str>) -> Option<&'static Rule> {
    let full = group.map(|g| format!("{g}:{name}"));
    let hit = |p: &str| {
        matches(p, name)
            || full.as_deref().is_some_and(|f| matches(p, f))
            || group.is_some_and(|g| matches(p, g))
    };
    // Exact names win over wildcard prefixes (e.g. `@nestjs/swagger` vs `@nestjs/*`).
    RULES
        .iter()
        .find(|r| r.names.iter().any(|p| !p.ends_with('*') && hit(p)))
        .or_else(|| {
            RULES
                .iter()
                .find(|r| r.names.iter().any(|p| p.ends_with('*') && hit(p)))
        })
}

fn lang_family(l: Language) -> Language {
    match l {
        Language::TypeScript => Language::JavaScript,
        Language::Kotlin => Language::Java,
        Language::Cpp => Language::C,
        other => other,
    }
}

pub fn std_hints_for(report: &Report) -> Vec<&'static StdHint> {
    let mut out: Vec<&'static StdHint> = vec![];
    for h in STD_HINTS {
        let used = report.files.iter().any(|f| {
            lang_family(f.language) == h.lang
                && f.imports.iter().any(|i| {
                    let i = i.trim_start_matches("node:");
                    i == h.module
                        || i.starts_with(&format!("{}.", h.module))
                        || i.starts_with(&format!("{}/", h.module))
                })
        });
        if used {
            out.push(h);
        }
    }
    out
}

/// A resolved dependency -> crate mapping row.
#[derive(Debug, Clone, Serialize)]
pub struct Mapped {
    pub source: String,
    pub ecosystem: String,
    pub dev: bool,
    pub category: Option<Category>,
    pub crates: Vec<CrateRec>,
    pub note: String,
}

pub fn map_dependencies(report: &Report) -> Vec<Mapped> {
    let mut out: Vec<Mapped> = report
        .dependencies
        .iter()
        .filter(|d| d.ecosystem != "cargo")
        .map(|d| {
            let rule = lookup(&d.name, d.group.as_deref());
            Mapped {
                source: match &d.group {
                    Some(g) => format!("{g}:{}", d.name),
                    None => d.name.clone(),
                },
                ecosystem: d.ecosystem.clone(),
                dev: d.dev,
                category: rule.map(|r| r.category),
                crates: rule.map(|r| r.crates.to_vec()).unwrap_or_default(),
                note: rule.map(|r| r.note.to_string()).unwrap_or_default(),
            }
        })
        .collect();
    // Imports of well-known libraries not declared in a manifest (e.g. a lone script).
    let declared: BTreeSet<String> = report
        .dependencies
        .iter()
        .map(|d| normalize(&d.name))
        .collect();
    let mut seen = BTreeSet::new();
    for f in &report.files {
        for imp in &f.imports {
            if imp.starts_with('.') || imp.starts_with('/') {
                continue;
            }
            let root = match f.language {
                Language::Python => imp.split('.').next().unwrap_or(imp).to_string(),
                Language::JavaScript | Language::TypeScript => {
                    if imp.starts_with('@') {
                        imp.splitn(3, '/').take(2).collect::<Vec<_>>().join("/")
                    } else {
                        imp.split('/').next().unwrap_or(imp).to_string()
                    }
                }
                Language::Go => imp.clone(),
                _ => continue,
            };
            if declared.contains(&normalize(&root)) || !seen.insert(normalize(&root)) {
                continue;
            }
            if let Some(rule) = lookup(&root, None) {
                out.push(Mapped {
                    source: root,
                    ecosystem: "import".into(),
                    dev: false,
                    category: Some(rule.category),
                    crates: rule.crates.to_vec(),
                    note: rule.note.to_string(),
                });
            }
        }
    }
    out
}

pub fn detect_frameworks(report: &Report) -> Vec<String> {
    let mut found: BTreeSet<String> = BTreeSet::new();
    let interesting = [
        Category::WebFramework,
        Category::Orm,
        Category::Frontend,
        Category::Desktop,
        Category::Api,
        Category::DataScience,
        Category::MachineLearning,
        Category::Messaging,
        Category::Cli,
    ];
    for d in &report.dependencies {
        if let Some(r) = lookup(&d.name, d.group.as_deref()) {
            if interesting.contains(&r.category) && !d.dev {
                if let Some(p) = pretty_framework(&d.name) {
                    found.insert(p);
                }
            }
        }
    }
    for f in &report.files {
        for r in &f.routes {
            found.insert(match r.framework.as_str() {
                "flask" => "Flask".into(),
                "fastapi" => "FastAPI".into(),
                "django" => "Django".into(),
                "express" => "Express".into(),
                "nestjs" => "NestJS".into(),
                "spring" => "Spring".into(),
                "aspnet" => "ASP.NET Core".into(),
                "net/http" => "Go net/http".into(),
                "laravel" => "Laravel".into(),
                "gin/echo/chi" => continue,
                other => other.to_string(),
            });
        }
    }
    found.into_iter().collect()
}

fn pretty_framework(name: &str) -> Option<String> {
    let n = normalize(name);
    let pretty = [
        ("express", "Express"),
        ("fastify", "Fastify"),
        ("koa", "Koa"),
        ("@nestjs/core", "NestJS"),
        ("next", "Next.js"),
        ("flask", "Flask"),
        ("django", "Django"),
        ("fastapi", "FastAPI"),
        ("spring-boot-starter-web", "Spring"),
        ("spring-boot-starter-webflux", "Spring WebFlux"),
        ("github.com/gin-gonic/gin", "Gin"),
        ("react", "React"),
        ("vue", "Vue"),
        ("@angular/core", "Angular"),
        ("svelte", "Svelte"),
        ("electron", "Electron"),
        ("rails", "Rails"),
        ("laravel/framework", "Laravel"),
        ("sqlalchemy", "SQLAlchemy"),
        ("prisma", "Prisma"),
        ("@prisma/client", "Prisma"),
        ("typeorm", "TypeORM"),
        ("sequelize", "Sequelize"),
        ("mongoose", "Mongoose"),
        ("pandas", "pandas"),
        ("spring-boot-starter-data-jpa", "Spring Data JPA"),
        ("hibernate-core", "Hibernate"),
        ("gorm.io/gorm", "GORM"),
        ("celery", "Celery"),
        ("graphql", "GraphQL"),
        ("@grpc/grpc-js", "gRPC"),
        ("grpcio", "gRPC"),
        ("torch", "PyTorch"),
        ("tensorflow", "TensorFlow"),
        ("scikit-learn", "scikit-learn"),
        ("click", "Click"),
        ("typer", "Typer"),
        ("commander", "Commander"),
        ("github.com/spf13/cobra", "Cobra"),
        ("github.com/labstack/echo/v4", "Echo"),
        ("github.com/gofiber/fiber/v2", "Fiber"),
        ("github.com/go-chi/chi/v5", "chi"),
        ("sqlmodel", "SQLModel"),
        ("drizzle-orm", "Drizzle"),
        ("hono", "Hono"),
        ("sanic", "Sanic"),
        ("starlette", "Starlette"),
        ("tornado", "Tornado"),
        ("djangorestframework", "Django REST framework"),
        ("spring-boot-starter", "Spring Boot"),
        ("quarkus-resteasy", "Quarkus"),
        ("ktor-server-core", "Ktor"),
        ("javalin", "Javalin"),
        ("sinatra", "Sinatra"),
        ("gorm.io/gorm", "GORM"),
        ("microsoft.aspnetcore.app", "ASP.NET Core"),
        ("microsoft.entityframeworkcore", "Entity Framework Core"),
        ("polars", "Polars"),
        ("pyspark", "Spark"),
        ("numpy", "NumPy"),
        ("kafkajs", "Kafka"),
        ("kafka-python", "Kafka"),
        ("spring-kafka", "Kafka"),
        ("bullmq", "BullMQ"),
        ("pika", "RabbitMQ"),
        ("amqplib", "RabbitMQ"),
        ("@apollo/server", "Apollo GraphQL"),
        ("apollo-server", "Apollo GraphQL"),
        ("graphene", "Graphene"),
        ("strawberry-graphql", "Strawberry"),
        ("@nestjs/graphql", "GraphQL"),
        ("pyqt5", "PyQt"),
        ("pyqt6", "PyQt"),
        ("tkinter", "Tk"),
        ("@angular/core", "Angular"),
        ("solid-js", "Solid"),
        ("transformers", "Hugging Face Transformers"),
        ("langchain", "LangChain"),
        ("llama-index", "LlamaIndex"),
    ];
    pretty
        .iter()
        .find(|(k, _)| normalize(k) == n)
        .map(|(_, v)| v.to_string())
}

pub fn is_web_project(report: &Report) -> bool {
    !report.all_routes().is_empty()
        || report.dependencies.iter().any(|d| {
            !d.dev
                && lookup(&d.name, d.group.as_deref()).is_some_and(|r| {
                    r.category == Category::WebFramework && !r.names.contains(&"cors")
                })
        })
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Crates a generated type expression needs.
#[derive(Default, Debug)]
pub struct TypeNeeds {
    pub chrono: bool,
    pub uuid: bool,
    pub decimal: bool,
}

/// Map a source-language type annotation to a Rust type that compiles in the
/// scaffold. `known` holds the names of structs/enums the scaffold generates.
/// Names of generated structs/enums: original PascalCase name -> Rust name.
pub type Known = BTreeMap<String, String>;

pub fn rust_type(lang: Language, ty: Option<&str>, known: &Known, needs: &mut TypeNeeds) -> String {
    let Some(raw) = ty.map(str::trim).filter(|t| !t.is_empty()) else {
        return "serde_json::Value".into();
    };
    map_type(lang_family(lang), raw, known, needs, 0)
}

/// Split `A | B<C | D>` on top-level separators only.
fn split_union(t: &str, sep: char) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in t.chars() {
        match ch {
            '<' | '[' | '(' | '{' => depth += 1,
            '>' | ']' | ')' | '}' => depth -= 1,
            _ => {}
        }
        if ch == sep && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    out.push(cur);
    out
}

/// Types the scaffold can safely use as HashMap keys / HashSet elements
/// (they implement Hash + Eq + Ord + serde).
fn hashable(t: &str) -> bool {
    matches!(
        t,
        "String"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "bool"
            | "uuid::Uuid"
    )
}

fn key(t: String) -> String {
    if hashable(&t) {
        t
    } else {
        "String".into()
    }
}

fn generic_args(s: &str, open: char, close: char) -> Option<(String, Vec<String>)> {
    let i = s.find(open)?;
    if !s.ends_with(close) {
        return None;
    }
    let head = s[..i].trim().to_string();
    let inner = &s[i + 1..s.len() - 1];
    Some((head, crate::text::split_top_level(inner)))
}

fn map_type(
    lang: Language,
    raw: &str,
    known: &Known,
    needs: &mut TypeNeeds,
    depth: usize,
) -> String {
    if depth > 6 {
        return "serde_json::Value".into();
    }
    let t = raw.trim().trim_start_matches("readonly ").trim();
    let rec = |s: &str, needs: &mut TypeNeeds| map_type(lang, s, known, needs, depth + 1);
    let val = "serde_json::Value".to_string();

    // Unions with null/undefined/None -> Option.
    let union_sep =
        if lang == Language::Python || lang == Language::JavaScript || lang == Language::Php {
            Some('|')
        } else {
            None
        };
    if let Some(sep) = union_sep {
        let parts = split_union(t, sep);
        let parts: Vec<&str> = parts
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() > 1 {
            let non_null: Vec<&str> = parts
                .iter()
                .copied()
                .filter(|p| !matches!(*p, "null" | "undefined" | "None" | "void"))
                .collect();
            let inner = if non_null.len() == 1 {
                rec(non_null[0], needs)
            } else {
                val.clone()
            };
            return if non_null.len() < parts.len() {
                format!("Option<{inner}>")
            } else {
                inner
            };
        }
    }
    // Nullable suffix (C#, Kotlin, PHP prefix).
    if let Some(inner) = t.strip_suffix('?') {
        return format!("Option<{}>", rec(inner, needs));
    }
    if lang == Language::Php {
        if let Some(inner) = t.strip_prefix('?') {
            return format!("Option<{}>", rec(inner, needs));
        }
    }
    // Arrays.
    if let Some(inner) = t.strip_suffix("[]") {
        return if inner.trim() == "byte" {
            "Vec<u8>".into()
        } else {
            format!("Vec<{}>", rec(inner, needs))
        };
    }
    if lang == Language::Go {
        if let Some(inner) = t.strip_prefix("[]") {
            return if inner == "byte" {
                "Vec<u8>".into()
            } else {
                format!("Vec<{}>", rec(inner, needs))
            };
        }
        if let Some(rest) = t.strip_prefix("map[") {
            if let Some((k, v)) = rest.split_once(']') {
                return format!(
                    "std::collections::HashMap<{}, {}>",
                    key(rec(k, needs)),
                    rec(v, needs)
                );
            }
        }
        if let Some(inner) = t.strip_prefix('*') {
            return rec(inner, needs);
        }
        if t.starts_with("chan ") || t.starts_with("<-chan") || t.starts_with("func(") {
            return val;
        }
        // Go multiple returns: (T, error)
        if t.starts_with('(') && t.ends_with(')') {
            let parts = crate::text::split_top_level(&t[1..t.len() - 1]);
            let parts: Vec<String> = parts
                .iter()
                .map(|p| p.split_whitespace().last().unwrap_or(p).to_string())
                .collect();
            let has_err = parts.last().is_some_and(|p| p == "error");
            let vals: Vec<String> = parts
                .iter()
                .filter(|p| *p != "error")
                .map(|p| rec(p, needs))
                .collect();
            let inner = match vals.len() {
                0 => "()".to_string(),
                1 => vals[0].clone(),
                _ => format!("({})", vals.join(", ")),
            };
            return if has_err {
                format!("anyhow::Result<{inner}>")
            } else {
                inner
            };
        }
        if let Some(rest) = t.strip_prefix("...") {
            return format!("Vec<{}>", rec(rest, needs));
        }
    }
    if lang == Language::C {
        let base = t
            .replace("const ", "")
            .replace("unsigned ", "u_")
            .replace("struct ", "");
        let base = base.trim();
        if matches!(base, "char*" | "char *") {
            return "String".into();
        }
        if let Some(inner) = base.strip_suffix('*') {
            return match inner.trim() {
                "void" => val,
                i => format!("Option<Box<{}>>", rec(i, needs)),
            };
        }
        if let Some((inner, _)) = base.split_once('[') {
            return format!("Vec<{}>", rec(inner, needs));
        }
        if let Some(inner) = base.strip_suffix('&') {
            return rec(inner, needs);
        }
        if let Some(stripped) = base.strip_prefix("std::") {
            if let Some((head, args)) = generic_args(stripped, '<', '>') {
                return match head.as_str() {
                    "vector" | "list" | "deque" => format!("Vec<{}>", rec(&args[0], needs)),
                    "map" => format!(
                        "std::collections::BTreeMap<{}, {}>",
                        key(rec(&args[0], needs)),
                        rec(args.get(1).map(String::as_str).unwrap_or("int"), needs)
                    ),
                    "unordered_map" => format!(
                        "std::collections::HashMap<{}, {}>",
                        key(rec(&args[0], needs)),
                        rec(args.get(1).map(String::as_str).unwrap_or("int"), needs)
                    ),
                    "set" | "unordered_set" => {
                        let e = rec(&args[0], needs);
                        if hashable(&e) {
                            format!("std::collections::HashSet<{e}>")
                        } else {
                            format!("Vec<{e}>")
                        }
                    }
                    "optional" => format!("Option<{}>", rec(&args[0], needs)),
                    "shared_ptr" => format!("std::sync::Arc<{}>", rec(&args[0], needs)),
                    "unique_ptr" => format!("Box<{}>", rec(&args[0], needs)),
                    "pair" | "tuple" => format!(
                        "({})",
                        args.iter()
                            .map(|a| rec(a, needs))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    _ => val,
                };
            }
            if stripped == "string" {
                return "String".into();
            }
        }
    }

    // Generic forms Foo<...> / Foo[...]
    let generic = generic_args(t, '<', '>').or_else(|| {
        if lang == Language::Python {
            generic_args(t, '[', ']')
        } else {
            None
        }
    });
    if let Some((head, args)) = generic {
        let a = |i: usize, needs: &mut TypeNeeds| {
            args.get(i)
                .map(|x| rec(x, needs))
                .unwrap_or_else(|| "serde_json::Value".into())
        };
        let head_l = head.rsplit('.').next().unwrap_or(&head).to_string();
        return match head_l.as_str() {
            "List" | "list" | "ArrayList" | "LinkedList" | "Array" | "ReadonlyArray"
            | "Sequence" | "Iterable" | "Collection" | "IEnumerable" | "IList" | "ICollection"
            | "IReadOnlyList" | "MutableList" | "Stream" | "Flux" | "Vector" | "Deque"
            | "Queue" => format!("Vec<{}>", a(0, needs)),
            "Set" | "set" | "HashSet" | "TreeSet" | "frozenset" | "FrozenSet" | "ISet"
            | "MutableSet" => {
                let e = a(0, needs);
                if hashable(&e) {
                    format!("std::collections::HashSet<{e}>")
                } else {
                    format!("Vec<{e}>")
                }
            }
            "Map"
            | "map"
            | "dict"
            | "Dict"
            | "HashMap"
            | "Record"
            | "Mapping"
            | "MutableMapping"
            | "Dictionary"
            | "IDictionary"
            | "IReadOnlyDictionary"
            | "ConcurrentHashMap"
            | "MutableMap"
            | "DefaultDict"
            | "defaultdict"
            | "OrderedDict" => {
                format!(
                    "std::collections::HashMap<{}, {}>",
                    key(a(0, needs)),
                    a(1, needs)
                )
            }
            "TreeMap" | "SortedMap" | "SortedDictionary" => format!(
                "std::collections::BTreeMap<{}, {}>",
                key(a(0, needs)),
                a(1, needs)
            ),
            "Optional" | "Option" | "Maybe" | "Nullable" => format!("Option<{}>", a(0, needs)),
            "Promise" | "Future" | "CompletableFuture" | "Task" | "ValueTask" | "Mono"
            | "Awaitable" | "Coroutine" | "Deferred" | "Observable" => {
                let inner = args.last().cloned().unwrap_or_default();
                if inner.is_empty() || inner == "void" || inner == "Void" || inner == "None" {
                    "()".into()
                } else {
                    rec(&inner, needs)
                }
            }
            "ResponseEntity" | "ActionResult" | "IActionResult" | "Response" | "JSONResponse" => {
                a(0, needs)
            }
            "Tuple" | "tuple" | "Pair" | "Triple" => format!(
                "({})",
                args.iter()
                    .map(|x| rec(x, needs))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Union" => {
                let non_null: Vec<&String> = args
                    .iter()
                    .filter(|x| !matches!(x.as_str(), "None" | "null"))
                    .collect();
                let inner = if non_null.len() == 1 {
                    rec(non_null[0], needs)
                } else {
                    val.clone()
                };
                if non_null.len() < args.len() {
                    format!("Option<{inner}>")
                } else {
                    inner
                }
            }
            "Partial" | "Readonly" | "Required" | "Annotated" | "Final" | "ClassVar" => a(0, needs),
            "Callable" | "Function" | "Func" | "Action" | "Supplier" | "Consumer" => val,
            _ => rec(&head_l, needs),
        };
    }

    let name = t.rsplit(['.', '\\']).next().unwrap_or(t);
    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
        if let Some(rust) = known.get(&crate::scaffold::pascal(name)) {
            return rust.clone();
        }
    }
    match name {
        "int" | "Integer" | "Int" | "int32" | "Int32" | "i32" | "short" | "Short" | "int16"
        | "rune" => "i32".into(),
        "long" | "Long" | "int64" | "Int64" | "i64" | "BigInteger" | "BigInt" | "bigint"
        | "uint32" | "long long" => {
            if lang == Language::Go && name == "uint32" {
                "u32".into()
            } else {
                "i64".into()
            }
        }
        "uint" | "uint64" | "ulong" | "UInt64" | "size_t" | "usize" => {
            if name == "size_t" {
                "usize".into()
            } else {
                "u64".into()
            }
        }
        "u_int" | "u_int32_t" | "uint32_t" => "u32".into(),
        "uint8" | "byte" | "Byte" | "u8" | "uint8_t" | "u_char" => "u8".into(),
        "int8" | "sbyte" | "i8" | "int8_t" | "char"
            if lang != Language::Java && lang != Language::CSharp =>
        {
            "i8".into()
        }
        "int16_t" => "i16".into(),
        "int32_t" => "i32".into(),
        "int64_t" => "i64".into(),
        "uint16" | "uint16_t" | "ushort" => "u16".into(),
        "uint64_t" | "u_long" => "u64".into(),
        "float" | "Float" | "double" | "Double" | "float64" | "number" | "Number" | "f64"
        | "real" => {
            if lang == Language::Python
                || lang == Language::JavaScript
                || name.contains("64")
                || name.eq_ignore_ascii_case("double")
                || name == "real"
            {
                "f64".into()
            } else {
                "f32".into()
            }
        }
        "float32" | "f32" => "f32".into(),
        "decimal" | "Decimal" | "BigDecimal" | "Money" => {
            needs.decimal = true;
            "rust_decimal::Decimal".into()
        }
        "str" | "string" | "String" | "CharSequence" | "Text" | "char" | "Character" | "symbol"
        | "ID" | "EmailStr" | "HttpUrl" | "AnyUrl" => "String".into(),
        "bool" | "boolean" | "Boolean" | "Bool" => "bool".into(),
        "bytes" | "bytearray" | "Buffer" | "Uint8Array" | "ByteArray" | "ArrayBuffer" | "Blob" => {
            "Vec<u8>".into()
        }
        "void" | "None" | "Void" | "Unit" | "undefined" | "null" | "never" | "NoReturn" => {
            "()".into()
        }
        "error" | "Error" | "Exception" | "Throwable" => "anyhow::Error".into(),
        "datetime" | "Date" | "DateTime" | "LocalDateTime" | "OffsetDateTime" | "ZonedDateTime"
        | "Instant" | "Timestamp" | "DateTimeOffset" | "Time" | "time" => {
            needs.chrono = true;
            "chrono::DateTime<chrono::Utc>".into()
        }
        "date" | "LocalDate" | "DateOnly" => {
            needs.chrono = true;
            "chrono::NaiveDate".into()
        }
        "timedelta" | "Duration" | "TimeSpan" => "std::time::Duration".into(),
        "UUID" | "Uuid" | "Guid" | "uuid" => {
            needs.uuid = true;
            "uuid::Uuid".into()
        }
        "list" | "List" | "array" | "Array" | "ArrayList" | "tuple" => {
            "Vec<serde_json::Value>".into()
        }
        "dict" | "Dict" | "Map" | "map" | "object" | "Object" | "HashMap" | "Record" | "JSON"
        | "JsonNode" | "JObject" | "JsonElement" => {
            if matches!(name, "object" | "Object") {
                val
            } else {
                "std::collections::HashMap<String, serde_json::Value>".into()
            }
        }
        "set" | "Set" | "HashSet" => "std::collections::HashSet<String>".into(),
        "any" | "Any" | "unknown" | "interface{}" | "dynamic" | "mixed" | "T" | "K" | "V" | "E"
        | "TResult" => val,
        _ => val,
    }
}

// ---------------------------------------------------------------------------
// Concepts
// ---------------------------------------------------------------------------

pub fn concept_guidance(concept: &str) -> Option<(&'static str, &'static str)> {
    Some(match concept {
        "inheritance" => ("Class inheritance", "Rust has no inheritance. Shared behaviour becomes a `trait` with default methods; shared data becomes composition (a field holding the 'base' struct). Abstract classes map to traits."),
        "exceptions" => ("Exceptions / try-catch", "Return `Result<T, E>` and propagate with `?`. Use `thiserror` for library error enums and `anyhow` at application edges. `panic!` is only for bugs."),
        "error_values" => ("Go error values", "`(T, error)` maps directly to `Result<T, E>`; `if err != nil { return err }` becomes `?`."),
        "null_values" => ("null / None / nil", "Use `Option<T>`; the compiler forces every `None` case to be handled. `?.` chains become `.and_then()` / `?` on Options."),
        "async" => ("async / promises / futures", "Use async fn + `.await` on the tokio runtime. `Promise.all` -> `tokio::join!`/`futures::future::join_all`; timeouts -> `tokio::time::timeout`."),
        "threads" => ("Threads & locks", "Shared state becomes `Arc<Mutex<T>>` / `Arc<RwLock<T>>`; the compiler rejects data races. Thread pools -> rayon (CPU) or tokio (IO)."),
        "goroutines" => ("Goroutines", "`go f()` -> `tokio::spawn(async move { ... })`; `sync.WaitGroup` -> `JoinSet` or `join_all`."),
        "channels" => ("Channels", "Go channels map to `tokio::sync::mpsc` (many-to-one), `broadcast` (fan-out) or `oneshot` (single reply). `select {}` -> `tokio::select!`."),
        "defer" => ("defer", "Cleanup happens automatically when values go out of scope (`Drop`). For ad-hoc cleanup use a guard struct or the `scopeguard` crate."),
        "reflection" => ("Reflection / dynamic attributes", "Rust has no runtime reflection. Replace with traits, enums, `serde` (for field-by-name access) or derive macros. Review each use by hand."),
        "dynamic_eval" => ("eval / exec / metaprogramming", "Cannot be ported mechanically. Replace with an explicit interpreter, a match over allowed operations, or an embedded scripting engine (rhai, mlua) if users really supply code."),
        "any_type" => ("Untyped values (any/Object/interface{})", "Introduce concrete types or enums. Where data is genuinely dynamic (JSON blobs), use `serde_json::Value`; the scaffold uses it as a placeholder for unknown types."),
        "decorators" => ("Decorators / annotations", "Route decorators become router registrations; cross-cutting ones (auth, logging, retry, transactions) become tower middleware layers or explicit wrapper functions; validation becomes `#[derive(Validate)]`."),
        "global_state" => ("Global mutable state", "Pass state explicitly (axum `State<Arc<AppState>>`), or use `std::sync::OnceLock`/`LazyLock` for init-once globals. Mutable globals need a Mutex."),
        "generators" => ("Generators / yield", "Implement `Iterator` (sync) or return `impl Stream` (async, via `async-stream`). Many generators collapse into iterator adaptor chains."),
        "closures" => ("Closures / lambdas / callbacks", "Rust closures (`|x| ...`) work similarly; captured state needs `move` when the closure outlives the scope. Callback-heavy APIs often become async functions."),
        "mixins" => ("Mixins / modules", "Mixins become traits with default methods, implemented per type."),
        "iterators" => ("Streams / LINQ", "Java streams and LINQ map almost 1:1 to Iterator adaptors (`iter().filter().map().collect()`)."),
        "manual_memory" => ("Manual memory (malloc/free/new/delete)", "Ownership replaces manual frees: `Box<T>` for heap values, `Vec<T>` for buffers, `Rc/Arc` for shared ownership. No use-after-free or double free is possible in safe Rust."),
        "unsafe_pointers" => ("Raw pointers", "Replace with references (`&T`/`&mut T`), slices, indices into a Vec, or smart pointers. Keep `unsafe` only at FFI boundaries and document invariants."),
        "macros" => ("Preprocessor macros", "Constants -> `const`; function-like macros -> generic/inline functions or `macro_rules!`; `#ifdef` -> `#[cfg(...)]`."),
        "generics" => ("Templates", "C++ templates -> Rust generics with trait bounds; template specialisation -> traits implemented per type."),
        "goto" => ("goto", "Restructure with loops, labelled `break`/`continue` and early returns."),
        _ => return None,
    })
}

/// All distinct crates suggested for a report, merged by name (features unioned).
pub fn suggested_crates(report: &Report) -> BTreeMap<String, (String, BTreeSet<String>)> {
    let mut out: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    let mut add = |c: &CrateRec| {
        let e = out
            .entry(c.name.to_string())
            .or_insert_with(|| (c.version.to_string(), BTreeSet::new()));
        e.1.extend(c.features.iter().map(|f| f.to_string()));
    };
    for m in map_dependencies(report) {
        if m.dev {
            continue;
        }
        for c in &m.crates {
            add(c);
        }
    }
    for h in std_hints_for(report) {
        for c in h.crates {
            add(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert_eq!(lookup("express", None).unwrap().crates[0].name, "axum");
        assert_eq!(
            lookup("Flask", None).unwrap().category,
            Category::WebFramework
        );
        assert_eq!(
            lookup(
                "spring-boot-starter-data-jpa",
                Some("org.springframework.boot")
            )
            .unwrap()
            .category,
            Category::Orm
        );
        assert_eq!(
            lookup("@nestjs/swagger", None).unwrap().category,
            Category::Api
        );
        assert!(lookup("github.com/labstack/echo/v4", None).is_some());
        assert!(lookup("totally-unknown-package", None).is_none());
    }

    #[test]
    fn types() {
        let known: Known = [("User".to_string(), "User".to_string())]
            .into_iter()
            .collect();
        let mut n = TypeNeeds::default();
        let t = |l, s: &str, n: &mut TypeNeeds| rust_type(l, Some(s), &known, n);
        assert_eq!(
            t(Language::Python, "Optional[List[int]]", &mut n),
            "Option<Vec<i32>>"
        );
        assert_eq!(
            t(Language::Python, "dict[str, float]", &mut n),
            "std::collections::HashMap<String, f64>"
        );
        assert_eq!(
            t(Language::TypeScript, "Promise<User | null>", &mut n),
            "Option<User>"
        );
        assert_eq!(t(Language::TypeScript, "string[]", &mut n), "Vec<String>");
        assert_eq!(
            t(Language::Go, "(int, error)", &mut n),
            "anyhow::Result<i32>"
        );
        assert_eq!(
            t(Language::Go, "map[string][]byte", &mut n),
            "std::collections::HashMap<String, Vec<u8>>"
        );
        assert_eq!(
            t(Language::Java, "List<BigDecimal>", &mut n),
            "Vec<rust_decimal::Decimal>"
        );
        assert!(n.decimal);
        assert_eq!(t(Language::CSharp, "Task<int?>", &mut n), "Option<i32>");
        assert_eq!(
            t(Language::Cpp, "std::vector<std::string>", &mut n),
            "Vec<String>"
        );
        assert_eq!(t(Language::C, "const char*", &mut n), "String");
        assert_eq!(t(Language::Python, "Customer", &mut n), "serde_json::Value");
    }
}
