data "google_project" "this" {
  project_id = var.project_id
}

# Autopilot nodes run as the default compute SA; grant it image pulls from AR.
resource "google_project_iam_member" "ar_reader" {
  project = var.project_id
  role    = "roles/artifactregistry.reader"
  member  = "serviceAccount:${data.google_project.this.number}-compute@developer.gserviceaccount.com"
}
