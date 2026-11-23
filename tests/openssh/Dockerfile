FROM public.ecr.aws/docker/library/alpine:3.22
RUN apk add --no-cache openssh procps iproute2 \
    && adduser -D -s /bin/ash sparrow \
    && echo 'sparrow:sparrow-integration-only' | chpasswd \
    && ssh-keygen -A \
    && mkdir -p /home/sparrow/.ssh \
    && chown -R sparrow:sparrow /home/sparrow/.ssh
COPY sshd-test.conf /etc/ssh/sshd_config.d/sparrowshell-test.conf
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
