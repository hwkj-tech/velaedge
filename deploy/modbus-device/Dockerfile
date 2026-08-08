FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir --requirement requirements.txt

COPY server.py .

USER 10001:10001

EXPOSE 502

HEALTHCHECK --interval=5s --timeout=3s --start-period=3s --retries=3 \
    CMD ["python", "/app/server.py", "--healthcheck"]

ENTRYPOINT ["python", "/app/server.py"]
