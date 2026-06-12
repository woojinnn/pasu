terraform {
  backend "gcs" {
    bucket = "policy-engine-498313-dambi-tfstate"
    prefix = "policy-server/m2"
  }
}
