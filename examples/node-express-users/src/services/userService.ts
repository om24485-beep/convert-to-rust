import bcrypt from 'bcryptjs';
import jwt from 'jsonwebtoken';
import { Pool } from 'pg';
import { v4 as uuid } from 'uuid';
import { AuthToken, CreateUserInput, Role, User } from '../models/user';

export class UserService {
  private pool: Pool;
  private secret: string;

  constructor(pool: Pool, secret: string) {
    this.pool = pool;
    this.secret = secret;
  }

  async create(input: CreateUserInput): Promise<User> {
    const passwordHash = await bcrypt.hash(input.password, 10);
    const user: User = {
      id: uuid(),
      email: input.email.toLowerCase(),
      displayName: input.displayName,
      role: input.role ?? Role.Customer,
      passwordHash,
      createdAt: new Date(),
    };
    await this.pool.query(
      'INSERT INTO users (id, email, display_name, role, password_hash, created_at) VALUES ($1,$2,$3,$4,$5,$6)',
      [user.id, user.email, user.displayName, user.role, user.passwordHash, user.createdAt],
    );
    return user;
  }

  async findById(id: string): Promise<User | null> {
    const { rows } = await this.pool.query('SELECT * FROM users WHERE id = $1', [id]);
    return rows[0] ?? null;
  }

  async login(email: string, password: string): Promise<AuthToken | null> {
    const { rows } = await this.pool.query('SELECT * FROM users WHERE email = $1', [email.toLowerCase()]);
    const user: User | undefined = rows[0];
    if (!user || !(await bcrypt.compare(password, user.passwordHash))) {
      return null;
    }
    const token = jwt.sign({ sub: user.id, role: user.role }, this.secret, { expiresIn: 3600 });
    return { token, expiresIn: 3600 };
  }
}

export function isAdmin(user: User): boolean {
  return user.role === Role.Admin;
}
