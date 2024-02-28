#! /bin/sh

# First arg is the command
COMMAND=$1


# If up or empty then run the docker-compose up command


if [ -z "$COMMAND" ] || [ "$COMMAND" = "up" ]; then
  docker-compose -f deployment/environments/dev_local/docker-compose.yml up --build -d

  echo "Refer to the following table for the running containers ports:"
  docker container ls --format "table {{.Names}}\t{{.Ports}}" -a
fi

if [ "$COMMAND" = "test" ]; then
  docker-compose -f deployment/environments/test/docker-compose.yml up --build -d

  echo "Set up to send traces to datadog"
fi

if [ "$COMMAND" = "down" ]; then
  docker-compose -f deployment/environments/dev_local/docker-compose.yml -f deployment/environments/test/docker-compose.yml down
fi