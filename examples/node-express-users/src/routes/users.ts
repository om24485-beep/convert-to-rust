import { Router } from 'express';
import { Pool } from 'pg';
import { z } from 'zod';
import { UserService } from '../services/userService';

const service = new UserService(new Pool(), process.env.JWT_SECRET ?? 'dev-secret');
export const usersRouter = Router();

const createSchema = z.object({
  email: z.string().email(),
  displayName: z.string().min(1),
  password: z.string().min(8),
});

export async function createUser(req: any, res: any) {
  const input = createSchema.parse(req.body);
  const user = await service.create(input);
  res.status(201).json({ id: user.id, email: user.email });
}

export async function getUser(req: any, res: any) {
  const user = await service.findById(req.params.id);
  if (!user) return res.status(404).json({ error: 'not found' });
  res.json(user);
}

usersRouter.post('/', createUser);
usersRouter.get('/:id', getUser);
usersRouter.post('/login', async (req, res) => {
  const token = await service.login(req.body.email, req.body.password);
  if (!token) return res.status(401).json({ error: 'invalid credentials' });
  res.json(token);
});
