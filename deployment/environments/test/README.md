# Test on cloud

This is a load test meant to run on a cloud provider (GCP).

The Pulumi configuration will build the necessary containers, stand up the
server and the load tester on a container image in GCE, and run the test.

The metrics/traces are uploaded to Datadog to handle scaling.

## TODO

* Add measurment of requests per second recording on the client every iteration of the loop, either print it or send to prometheus.
* Collect Server Resource usage, as well as all the collected timings, and requests per second (to compare with others impls).
* Do the go version, then the elixir version.

Bonus:
* Run a test connection with n client connection creators (just for sanity). 
* consider https://stressgrid.com/ for load testing
* Add matchmaking logic interface and make a test one that just simulates some delay to figure out how a user should be "ranked" and rank them all the same
* Add N test load VMs and ramp it up to see how many requests per second we can handle.
