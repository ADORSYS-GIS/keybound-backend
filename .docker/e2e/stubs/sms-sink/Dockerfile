FROM node:20-alpine

WORKDIR /app

COPY package.json tsconfig.json ./

RUN yarn install

COPY src ./src

RUN yarn build

EXPOSE 8081

CMD ["yarn", "start"]
