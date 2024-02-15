#! /bin/sh

# First arg is the command
COMMAND=$1


# If up or empty then run the docker-compose up command


if [ -z "$COMMAND" ] || [ "$COMMAND" = "up" ]; then
  docker-compose -f deployment/docker-compose/docker-compose.yml -f deployment/docker-compose/docker-compose.dev.yml up --build -d

  echo "Refer to the following table for the running containers ports:"
  docker container ls --format "table {{.Names}}\t{{.Ports}}" -a
fi

if [ "$COMMAND" = "down" ]; then
  docker-compose -f deployment/docker-compose/docker-compose.yml -f deployment/docker-compose/docker-compose.dev.yml down
fi