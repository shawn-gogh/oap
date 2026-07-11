FROM node:24-bookworm-slim
WORKDIR /app
COPY package.json ./
RUN npm install --no-audit --no-fund
COPY src ./src
ENV PORT=8080
EXPOSE 8080
CMD ["node", "src/index.mjs"]
