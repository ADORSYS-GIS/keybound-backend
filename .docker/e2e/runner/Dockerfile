FROM node:20-alpine

WORKDIR /app

COPY package.json tsconfig.json vitest.config.ts ./

RUN yarn install

COPY src ./src

CMD ["yarn", "test:smoke"]
