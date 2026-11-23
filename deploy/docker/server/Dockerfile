FROM eclipse-temurin:8-jre

WORKDIR /app

COPY sparrowshell-server-0.1.0.jar /app/sparrowshell-server.jar

RUN mkdir -p /data/feedback \
    && chown -R 10001:10001 /app /data

USER 10001:10001

EXPOSE 8070

ENV SERVER_ADDRESS=0.0.0.0 \
    SERVER_PORT=8070 \
    SPARROWSHELL_FEEDBACK_STORAGE_DIR=/data/feedback \
    JAVA_TOOL_OPTIONS="-Dfile.encoding=UTF-8 -XX:MaxRAMPercentage=75.0"

ENTRYPOINT ["java", "-jar", "/app/sparrowshell-server.jar"]
