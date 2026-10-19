FROM public.ecr.aws/docker/library/node:24-bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends openssh-server sudo findutils coreutils util-linux procps ca-certificates \
  && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/bash rack \
  && echo 'rack ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/rack \
  && chmod 440 /etc/sudoers.d/rack
RUN mkdir -p /run/sshd /home/rack/.ssh
COPY keys/id_ed25519.pub keys/id_ed25519_enc.pub /tmp/
RUN cat /tmp/id_ed25519.pub /tmp/id_ed25519_enc.pub > /home/rack/.ssh/authorized_keys
RUN chown -R rack:rack /home/rack/.ssh \
  && chmod 700 /home/rack/.ssh \
  && chmod 600 /home/rack/.ssh/authorized_keys
RUN ssh-keygen -A
RUN mkdir -p /var/cache/apt/archives \
  && dd if=/dev/zero of=/var/cache/apt/archives/cache-demo.deb bs=1M count=12 status=none
RUN echo 'rack:racktest' | chpasswd
RUN printf '\nPasswordAuthentication yes\nPermitRootLogin no\nPubkeyAuthentication yes\nUsePAM no\n' >> /etc/ssh/sshd_config
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
