FROM ubuntu
RUN apt-get update && apt-get install -y curl
COPY ./bin/charms /usr/local/bin/
COPY ./prover/bin/charms /usr/local/bin/charms-prover
CMD ["charms", "server"]
