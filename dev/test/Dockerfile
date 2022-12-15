FROM node:19-alpine

WORKDIR /code
ADD script.js script.js
RUN npm install @xmtp/xmtp-js ethers
CMD ["node", "script.js"]
