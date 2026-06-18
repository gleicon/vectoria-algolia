FROM node:22-alpine
WORKDIR /app
COPY package.json ./
RUN npm install --no-progress
COPY . .
CMD ["npm", "test"]
