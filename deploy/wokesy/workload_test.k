_released = resources(Release {image_version = "1.2.3"})
_pod = _released[0].spec.template.spec

test_kubernetes_adds_no_service_variables_to_the_environment = lambda {
    assert _pod.enableServiceLinks == False
}
