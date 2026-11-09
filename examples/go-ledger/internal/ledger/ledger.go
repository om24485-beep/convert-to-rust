package ledger

import (
	"errors"
	"sync"
	"time"

	"github.com/google/uuid"
)

type Account struct {
	ID        string    `json:"id"`
	Owner     string    `json:"owner"`
	Balance   int64     `json:"balance_paise"`
	CreatedAt time.Time `json:"created_at"`
}

type Transfer struct {
	From   string `json:"from"`
	To     string `json:"to"`
	Amount int64  `json:"amount_paise"`
}

type Book struct {
	mu       sync.RWMutex
	accounts map[string]*Account
	queue    chan Transfer
}

var ErrNotFound = errors.New("account not found")

func NewBook() *Book {
	return &Book{accounts: map[string]*Account{}, queue: make(chan Transfer, 100)}
}

func (b *Book) Open(owner string) *Account {
	b.mu.Lock()
	defer b.mu.Unlock()
	a := &Account{ID: uuid.NewString(), Owner: owner, CreatedAt: time.Now()}
	b.accounts[a.ID] = a
	return a
}

func (b *Book) Get(id string) (*Account, error) {
	b.mu.RLock()
	defer b.mu.RUnlock()
	a, ok := b.accounts[id]
	if !ok {
		return nil, ErrNotFound
	}
	return a, nil
}

func (b *Book) Submit(t Transfer) error {
	if t.Amount <= 0 {
		return errors.New("amount must be positive")
	}
	b.queue <- t
	return nil
}

func (b *Book) Run() {
	for t := range b.queue {
		b.apply(t)
	}
}

func (b *Book) apply(t Transfer) {
	b.mu.Lock()
	defer b.mu.Unlock()
	from, ok1 := b.accounts[t.From]
	to, ok2 := b.accounts[t.To]
	if !ok1 || !ok2 || from.Balance < t.Amount {
		return
	}
	from.Balance -= t.Amount
	to.Balance += t.Amount
}
