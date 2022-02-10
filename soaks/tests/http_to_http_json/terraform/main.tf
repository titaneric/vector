# Set up the providers needed to run this soak and other terraform related
# business. Here we only require 'kubernetes' to interact with the soak
# minikube.
terraform {
  required_providers {
    kubernetes = {
      version = "~> 2.5.0"
      source  = "hashicorp/kubernetes"
    }
  }
}

# Rig the kubernetes provider to communicate with minikube. The details of
# adjusting `~/.kube/config` are addressed by the soak control scripts.
provider "kubernetes" {
  config_path = "~/.kube/config"
}

# Setup background monitoring details. These are needed by the soak control to
# understand what vector et al's running behavior is.
module "monitoring" {
  source          = "../../../common/terraform/modules/monitoring"
  experiment_name = var.experiment_name
  variant         = var.type
  vector_image    = var.vector_image
  depends_on      = [module.vector, module.http-gen, module.http-blackhole]
}

# Setup the soak pieces
#
# This soak config sets up a vector soak with lading/http-gen feeding into vector,
# lading/http-blackhole receiving.
resource "kubernetes_namespace" "soak" {
  metadata {
    name = "soak"
  }
}

module "vector" {
  source       = "../../../common/terraform/modules/vector"
  type         = var.type
  vector_image = var.vector_image
  vector-toml  = file("${path.module}/vector.toml")
  namespace    = kubernetes_namespace.soak.metadata[0].name
  vector_cpus  = var.vector_cpus
  depends_on   = [module.http-blackhole]
}
module "http-blackhole" {
  source              = "../../../common/terraform/modules/lading_http_blackhole"
  type                = var.type
  http-blackhole-yaml = file("${path.module}/../../../common/configs/http_blackhole.yaml")
  namespace           = kubernetes_namespace.soak.metadata[0].name
  lading_image        = var.lading_image
}
module "http-gen" {
  source        = "../../../common/terraform/modules/lading_http_gen"
  type          = var.type
  http-gen-yaml = file("${path.module}/../../../common/configs/http_gen_http_source_json.yaml")
  namespace     = kubernetes_namespace.soak.metadata[0].name
  lading_image  = var.lading_image
  depends_on    = [module.vector]
}
