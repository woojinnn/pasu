resource "random_password" "db" {
  length  = 24
  special = false # alphanumeric → safe inside a postgres:// URL without escaping
}

resource "google_sql_database_instance" "dambi" {
  name                = "dambi-pg"
  database_version    = "POSTGRES_16"
  region              = var.region
  deletion_protection = false

  # Private IP requires the peering to exist first.
  depends_on = [google_service_networking_connection.psa]

  settings {
    tier                        = var.db_tier
    edition                     = "ENTERPRISE" # db-custom-* tiers require ENTERPRISE (not ENTERPRISE_PLUS)
    availability_type           = "ZONAL"
    deletion_protection_enabled = false # second flag; both must be false to destroy

    ip_configuration {
      ipv4_enabled    = false # no public IP
      private_network = google_compute_network.vpc.id
    }

    database_flags {
      name  = "max_connections"
      value = var.db_max_connections
    }
  }
}

resource "google_sql_database" "dambi" {
  name     = "dambi"
  instance = google_sql_database_instance.dambi.name
}

resource "google_sql_user" "dambi" {
  name     = "dambi"
  instance = google_sql_database_instance.dambi.name
  password = random_password.db.result
}
