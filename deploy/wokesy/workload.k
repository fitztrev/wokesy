import regex
import k8s.api.apps.v1 as apps
import k8s.api.core.v1 as core

IMAGE_REPOSITORY = "ghcr.io/lakinwecker/wokesy"
HTTP_PORT_NAME = "http"
HTTP_PORT = 8080
UNPRIVILEGED_ID = 65534

schema Release:
    image_version: str
    secret_name: str = "wokesy"

    check:
        regex.match(image_version, r"^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$"), "image_version is MAJOR.MINOR.PATCH without the v"

http_probe = lambda path: str, period: int, failures: int -> core.Probe {
    core.Probe {
        httpGet = core.HTTPGetAction {
            path = path
            port = HTTP_PORT_NAME
        }
        periodSeconds = period
        timeoutSeconds = 1
        failureThreshold = failures
    }
}

resources = lambda release: Release -> [apps.Deployment] {
    _labels = {"app.kubernetes.io/name": "wokesy"}
    _image = "${IMAGE_REPOSITORY}:${release.image_version}"
    _environment = [core.EnvFromSource {secretRef = core.SecretEnvSource {name = release.secret_name}}]
    _hardened = core.SecurityContext {
        allowPrivilegeEscalation = False
        readOnlyRootFilesystem = True
        capabilities = core.Capabilities {drop = ["ALL"]}
    }
    [
        apps.Deployment {
            metadata = {name = "wokesy", labels = _labels}
            spec = apps.DeploymentSpec {
                replicas = 1
                strategy = apps.DeploymentStrategy {type = "Recreate"}
                selector = {matchLabels = _labels}
                template = core.PodTemplateSpec {
                    metadata = {labels = _labels}
                    spec = core.PodSpec {
                        terminationGracePeriodSeconds = 30
                        enableServiceLinks = False
                        securityContext = core.PodSecurityContext {
                            runAsNonRoot = True
                            runAsUser = UNPRIVILEGED_ID
                            runAsGroup = UNPRIVILEGED_ID
                            seccompProfile = core.SeccompProfile {type = "RuntimeDefault"}
                        }
                        containers = [
                            core.Container {
                                name = "wokesy"
                                image = _image
                                ports = [core.ContainerPort {name = HTTP_PORT_NAME, containerPort = HTTP_PORT}]
                                envFrom = _environment
                                livenessProbe = http_probe("/livez", 10, 3)
                                readinessProbe = http_probe("/readyz", 5, 2)
                                resources = core.ResourceRequirements {
                                    requests = {cpu = "50m", memory = "64Mi"}
                                    limits = {memory = "256Mi"}
                                }
                                securityContext = _hardened
                                volumeMounts = [core.VolumeMount {name = "tmp", mountPath = "/tmp"}]
                            }
                        ]
                        volumes = [core.Volume {name = "tmp", emptyDir = core.EmptyDirVolumeSource {}}]
                    }
                }
            }
        }
    ]
}
