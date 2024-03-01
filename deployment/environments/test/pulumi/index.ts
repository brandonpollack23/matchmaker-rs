import * as docker from "@pulumi/docker";
import * as gcp from "@pulumi/gcp";
import * as pulumi from "@pulumi/pulumi";
import * as fs from "fs";

// Import the program's configuration settings.
const config = new pulumi.Config();
const buildContainers = config.getBoolean("buildContainers") ?? false;
const machineType = config.get("machineType") ?? "f1-micro";
const osImage = config.get("osImage") ?? "cos-cloud/cos-stable";
const instanceTag = config.get("instanceTag") ?? "webserver";
const servicePort = config.get("servicePort") ?? "1337";
const datadogApiKey = config.get("datadog-api-key");

// Create all the necessary docker image resources.
const matchmakerServerImage = buildContainers
  ? new docker.Image("matchmaker-server-image", {
      imageName: "docker.io/brandonpollack23/matchmaker-rs:latest",
      build: {
        platform: "linux/amd64",
        context: "../../../../",
        dockerfile: "../../../docker/matchmaker-rs.Dockerfile",
      },
    })
  : undefined;

const loadTestClientImage = buildContainers
  ? new docker.Image("matchmaker-test-client-image", {
      imageName: "docker.io/brandonpollack23/matchmaker-load-test-client:latest",
      build: {
        platform: "linux/amd64",
        context: "../../../../",
        dockerfile: "../../../docker/load_tester_client.Dockerfile",
      },
    })
  : undefined;

// Create a new network for the virtual machine.
const network = new gcp.compute.Network("network", {
  autoCreateSubnetworks: false,
});

// Create a subnet on the network.
const subnet = new gcp.compute.Subnetwork("subnet", {
  ipCidrRange: "10.0.1.0/24",
  network: network.id,
});

// Create a firewall allowing inbound access over ports 1337 (for matchmaker service) and 22 (for SSH).
const firewall = new gcp.compute.Firewall("firewall", {
  network: network.selfLink,
  allows: [
    {
      protocol: "tcp",
      ports: ["22", servicePort],
    },
  ],
  direction: "INGRESS",
  sourceRanges: ["0.0.0.0/0"],
  targetTags: [instanceTag],
});

// Create the servers (matchmaker, otel, etc)
const otelCollectorConfig = fs
  .readFileSync("../otel-collector-config-connector.yml", "utf8")
  .replace(/`/g, "\\`");

const otlpCollector = new gcp.compute.Instance(
  "otel-collector",
  {
    name: "matchmaker-otel-collector",
    machineType,
    bootDisk: {
      initializeParams: {
        image: osImage,
      },
    },
    networkInterfaces: [
      {
        network: network.id,
        subnetwork: subnet.id,
        accessConfigs: [{}],
      },
    ],
    serviceAccount: {
      scopes: ["https://www.googleapis.com/auth/cloud-platform"],
    },
    allowStoppingForUpdate: true,
    metadataStartupScript: pulumi.interpolate`#!/bin/bash
cat << 'EOF' > /home/chronos/otel-collector-config-connector.yml
${otelCollectorConfig}
EOF

docker run -d \
--restart always \
-p 4317:4317 -p 4318:4318 \
-e DD_API_KEY=${datadogApiKey} \
-e DD_SITE=us3.datadoghq.com \
-v /home/chronos/otel-collector-config-connector.yml:/etc/otelcol/otel-collector-config.yml \
otel/opentelemetry-collector-contrib:0.95.0 --config /etc/otelcol/otel-collector-config.yml
    `,
    tags: [instanceTag],
  },
  { dependsOn: [firewall] }
);

const matchmakerDependencies = buildContainers
  ? [firewall, otlpCollector, matchmakerServerImage!!, loadTestClientImage!!]
  : [firewall, otlpCollector];
const matchmakerRsServer = new gcp.compute.Instance(
  "matchmaker-rs",
  {
    machineType,
    bootDisk: {
      initializeParams: {
        image: osImage,
      },
    },
    networkInterfaces: [
      {
        network: network.id,
        subnetwork: subnet.id,
        accessConfigs: [{}],
      },
    ],
    serviceAccount: {
      scopes: ["https://www.googleapis.com/auth/cloud-platform"],
    },
    allowStoppingForUpdate: true,
    // https://www.pulumi.com/ai/answers/7JU7Bf3qufFVSHmxGbBg57/deploying-docker-containers-with-google-cloud-compute
    metadata: {
      "gce-container-declaration": pulumi.interpolate`
spec:
  containers:
    - name: 'matchmaker-rs'
      image: brandonpollack23/matchmaker-rs:latest
      ports:
        - containerPort: ${servicePort}
          hostPort: ${servicePort}
          protocol: TCP
      args:
        - '--otlp-endpoint'
        - http://${otlpCollector.networkInterfaces.apply(
          (interfaces) => interfaces[0].accessConfigs![0].natIp
        )}:4317
      restartPolicy: Always`,
    },
    tags: [instanceTag],
  },
  { dependsOn: matchmakerDependencies }
);

export const matchmakerDockerImage = matchmakerServerImage?.imageName;
export const matchmakerDockerImageUrl = pulumi.interpolate`https://hub.docker.com/repository/docker/brandonpollack23/matchmaker-rs/general`;
export const matchmakerInstanceName = matchmakerRsServer.name;
export const matchmakerServerIp = matchmakerRsServer.networkInterfaces.apply((interfaces) => {
  return interfaces[0].accessConfigs![0].natIp;
});
export const matchmakerServerURI = pulumi.interpolate`${matchmakerRsServer.networkInterfaces.apply(
  (interfaces) => {
    return interfaces[0].accessConfigs![0].natIp;
  }
)}:${servicePort}`;

export const otelServerIp = otlpCollector.networkInterfaces.apply((interfaces) => {
  return interfaces[0].accessConfigs![0].natIp;
});

export const loadTestClientDockerImage = loadTestClientImage?.imageName;
export const loadTestClientDockerImageUrl = pulumi.interpolate`https://hub.docker.com/repository/docker/brandonpollack23/matchmaker-load-test-client/general`;
