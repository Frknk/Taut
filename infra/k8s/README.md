# infra/k8s (semana 8)

Manifiestos de Kubernetes con Kustomize: `base/` más `overlays/staging` y `overlays/prod`.
En local se usa k3d; en la nube, el clúster gestionado que crea Terraform.

Por servicio: un Deployment y un Service. Además: HorizontalPodAutoscaler para los checker
workers (métrica: profundidad de la cola `checks.requested`), Ingress con TLS de Let's Encrypt,
NetworkPolicy que solo deja a los servicios hablar con sus bases de datos, y secretos desde el
gestor del proveedor vía External Secrets. Las migraciones corren en un Job previo al despliegue.

Compuerta: el MVP corriendo en un clúster para el Examen de Medio Ciclo.
