export enum Role {
  Admin = 'admin',
  Officer = 'officer',
  Customer = 'customer',
}

export interface User {
  id: string;
  email: string;
  displayName: string;
  role: Role;
  passwordHash: string;
  createdAt: Date;
  lastLoginAt?: Date;
}

export interface CreateUserInput {
  email: string;
  displayName: string;
  password: string;
  role?: Role;
}

export interface AuthToken {
  token: string;
  expiresIn: number;
}
