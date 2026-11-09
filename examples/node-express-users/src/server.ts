import express from 'express';
import winston from 'winston';
import { usersRouter } from './routes/users';

const logger = winston.createLogger({ transports: [new winston.transports.Console()] });
const app = express();
app.use(express.json());

app.get('/health', (_req, res) => res.json({ status: 'ok' }));
app.use('/users', usersRouter);

const port = Number(process.env.PORT ?? 3000);
app.listen(port, () => logger.info(`listening on ${port}`));
