terraform {
  backend "gcs" {
    bucket = "policy-engine-498313-pasu-tfstate"
    prefix = "policy-server/m2"
  }
}
