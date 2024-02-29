import * as docker from "@pulumi/docker";
import * as gcp from "@pulumi/gcp";
import * as pulumi from "@pulumi/pulumi";

// Import the program's configuration settings.
const config = new pulumi.Config();
const buildContainers = config.getBoolean("buildContainers") ?? true;
const machineType = config.get("machineType") ?? "f1-micro";
const osImage = config.get("osImage") ?? "cos-cloud/cos-stable";
const instanceTag = config.get("instanceTag") ?? "webserver";
const servicePort = config.get("servicePort") ?? "1337";
const datadogApiKey = config.getSecret("datadog-api-key");

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

// Create the matchmaker server
const serverDependencies = buildContainers
  ? [firewall, matchmakerServerImage!!, loadTestClientImage!!]
  : [firewall];
const instance = new gcp.compute.Instance(
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
      restartPolicy: Always`,
    },
    tags: [instanceTag],
  },
  { dependsOn: serverDependencies }
);

const instanceIP = instance.networkInterfaces.apply((interfaces) => {
  return interfaces[0].accessConfigs![0].natIp;
});

// export const matchmakerDockerImage = matchmakerServerImage.imageName;
export const matchmakerDockerImageUrl = pulumi.interpolate`https://hub.docker.com/repository/docker/brandonpollack23/matchmaker-rs/general`;
export const matchmakerInstanceName = instance.name;
export const matchmakerServerIp = instanceIP;
export const matchmakerServerURI = pulumi.interpolate`${instanceIP}:${servicePort}`;

// export const loadTestClientDockerImage = loadTestClientImage.imageName;
export const loadTestClientDockerImageUrl = pulumi.interpolate`https://hub.docker.com/repository/docker/brandonpollack23/matchmaker-load-test-client/general`;
