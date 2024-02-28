# Test on cloud

This is a load test meant to run on a cloud provider (GCP).

The Pulumi configuration will build the necessary containers, stand up the
server and the load tester on a container image in GCE, and run the test.

The metrics/traces are uploaded to Datadog to handle scaling.

# TODO

* Get this working locally to upload to Datadog.
* Make the Pulumi project that does all this
