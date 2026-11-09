package main

import (
	"net/http"

	"github.com/example/go-ledger/internal/ledger"
	"github.com/gin-gonic/gin"
)

func main() {
	book := ledger.NewBook()
	go book.Run()

	r := gin.Default()
	r.GET("/accounts/:id", func(c *gin.Context) {
		acct, err := book.Get(c.Param("id"))
		if err != nil {
			c.JSON(http.StatusNotFound, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, acct)
	})
	r.POST("/accounts", createAccount(book))
	r.POST("/transfers", postTransfer(book))
	r.Run(":8080")
}

func createAccount(book *ledger.Book) gin.HandlerFunc {
	return func(c *gin.Context) {
		var req struct{ Owner string }
		if err := c.BindJSON(&req); err != nil {
			return
		}
		c.JSON(http.StatusCreated, book.Open(req.Owner))
	}
}

func postTransfer(book *ledger.Book) gin.HandlerFunc {
	return func(c *gin.Context) {
		var t ledger.Transfer
		if err := c.BindJSON(&t); err != nil {
			return
		}
		if err := book.Submit(t); err != nil {
			c.JSON(http.StatusUnprocessableEntity, gin.H{"error": err.Error()})
			return
		}
		c.Status(http.StatusAccepted)
	}
}
