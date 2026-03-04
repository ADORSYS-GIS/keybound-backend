FROM node:20-alpine

WORKDIR /app

COPY package.json tsconfig.json vitest.config.ts ./

RUN npm install

COPY src ./src

CMD ["npm", "run", "test:smoke"]
